//! The debug session: `lldb-dap` (or the adapter that `TRCODE_DEBUG_ADAPTER` names) is started,
//! the program is launched under it and, when the adapter asks (`runInTerminal`), started in the
//! terminal tab of its run configuration, the breakpoints are handed over, and what the adapter
//! says is kept for the pane: where the program stopped, the stack, the variables.
//!
//! Everything is asked without waiting: a request is kept with the reply that will come
//! ([`Waiting`]), and what the adapter does is looked at when it says so ([`Event::DebugChanged`]).
//! The adapter may send an event before the answer to the request that caused it, so nothing is
//! assumed of their order. See "Run configurations and a debugger" in `docs/design.md`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::App;
use crate::breakpoints::Answer;
use crate::components::debug::model::{Frame, Incoming, Read, Status};
use crate::dap::{
    Capabilities, Client, ContinueResponse, DapError, EvaluateResponse, Event as DapEvent,
    InitializeArguments, Nothing, PendingReply, ReverseRequest, RunInTerminalArguments,
    RunInTerminalResponse, ScopesResponse, SetBreakpointsArguments, SetBreakpointsResponse, Source,
    SourceBreakpoint, StackTraceArguments, StackTraceResponse, Stopped, ThreadsResponse, Update,
    VariablesArguments, VariablesResponse,
};
use crate::event::Event;
use crate::launch::{ConfigId, RunConfig, What, run_env};
use crate::pty::{Feeder, Session, Shell, SpawnConfig};
use crate::ui::ComponentKind;

/// How long an answer is waited for.
const PATIENCE: Duration = Duration::from_secs(30);
/// How long the adapter has to start the program (it may have to read a lot of debug information).
const LAUNCH_PATIENCE: Duration = Duration::from_secs(120);
/// How long the end of the session is waited for before the adapter is left.
const DISCONNECT_PATIENCE: Duration = Duration::from_secs(3);
/// How many frames of the stack are asked for.
const FRAMES: i64 = 50;

/// How an adapter is started: given what to call when it says something. Tests have their own.
pub(crate) type Adapter =
    Arc<dyn Fn(Box<dyn Fn() + Send>) -> Result<Client, DapError> + Send + Sync>;

/// What the debugger keeps in the app.
#[derive(Default)]
pub(super) struct Debugging {
    session: Option<Active>,
    adapter: Option<Adapter>,
    /// The expression the hover box waits for the value of.
    evaluating: evaluate::Evaluating,
    /// The session is being ended to start another one.
    restart: bool,
    /// The user was told that the layout has no Debug view.
    told_no_view: bool,
}

impl std::fmt::Debug for Debugging {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Debugging")
            .field("active", &self.session.is_some())
            .finish_non_exhaustive()
    }
}

pub(super) mod evaluate;
mod watch;

/// Which tree the children asked for are for.
#[derive(Clone, Copy)]
enum Target {
    Pane,
    Hover,
    Watch,
}

/// A request that was sent and is waiting for its answer.
struct Waiting {
    at: Instant,
    kind: Kind,
}

enum Kind {
    Initialize(PendingReply<Capabilities>),
    Launch(PendingReply<Nothing>),
    SetBreakpoints {
        path: PathBuf,
        reply: PendingReply<SetBreakpointsResponse>,
    },
    ConfigurationDone(PendingReply<Nothing>),
    Threads(PendingReply<ThreadsResponse>),
    Stack(PendingReply<StackTraceResponse>),
    Scopes {
        frame: i64,
        reply: PendingReply<ScopesResponse>,
    },
    Variables {
        row: u64,
        start: i64,
        target: Target,
        reply: PendingReply<VariablesResponse>,
    },
    Evaluate {
        ticket: u64,
        reply: PendingReply<EvaluateResponse>,
    },
    /// What a watch comes to.
    Watch {
        expression: String,
        reply: PendingReply<EvaluateResponse>,
    },
    Continue(PendingReply<ContinueResponse>),
    Step(&'static str, PendingReply<Nothing>),
    Disconnect(PendingReply<Nothing>),
}

impl Kind {
    fn limit(&self) -> Duration {
        match self {
            Self::Launch(_) => LAUNCH_PATIENCE,
            Self::Disconnect(_) => DISCONNECT_PATIENCE,
            _ => PATIENCE,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Initialize(_) => "initialize",
            Self::Launch(_) => "launch",
            Self::SetBreakpoints { .. } => "setBreakpoints",
            Self::ConfigurationDone(_) => "configurationDone",
            Self::Threads(_) => "threads",
            Self::Stack(_) => "stackTrace",
            Self::Scopes { .. } => "scopes",
            Self::Variables { .. } => "variables",
            Self::Evaluate { .. } | Self::Watch { .. } => "evaluate",
            Self::Continue(_) => "continue",
            Self::Step(name, _) => name,
            Self::Disconnect(_) => "disconnect",
        }
    }
}

/// A session that is going.
struct Active {
    client: Client,
    config: ConfigId,
    /// The arguments of `launch`, until the adapter has said what it can do.
    launch: Option<Value>,
    /// Where the build of the program was written, until the program is started (or the session
    /// ends before that).
    feeder: Feeder,
    /// The adapter said `initialized`: it takes the breakpoints.
    initialized: bool,
    /// `configurationDone` was sent: the program may run.
    configured: bool,
    /// The thread that stopped last.
    thread: Option<i64>,
    waiting: Vec<Waiting>,
    /// Why the session is to end, when something made it impossible to go on.
    ending: Option<String>,
}

impl App {
    /// Whether a debug session is going.
    pub(super) fn debug_active(&self) -> bool {
        self.debug.session.is_some()
    }

    /// Starts the adapter and the session for `config`, whose program is built at `program`.
    pub(super) fn start_debug(&mut self, config: &RunConfig, program: PathBuf, feeder: Feeder) {
        let Some(events) = self.background.sender() else {
            self.state
                .notify_error("debugging needs the event loop, which is not running");
            return;
        };
        let adapter = self.debug.adapter.clone().unwrap_or_else(default_adapter);
        let wake: Box<dyn Fn() + Send> = {
            let events = events.clone();
            Box::new(move || {
                let _ = events.blocking_send(Event::DebugChanged);
            })
        };
        let client = match adapter(wake) {
            Ok(client) => client,
            Err(error) => {
                let message = adapter_trouble(&error);
                feeder.text(format!("\n[{message}]\n").as_bytes());
                feeder.finish(127);
                self.state.notify_error(message);
                return;
            }
        };
        let initialize = match client.initialize(&InitializeArguments::new("lldb-dap")) {
            Ok(reply) => reply,
            Err(error) => {
                feeder.text(format!("\n[{error}]\n").as_bytes());
                feeder.finish(127);
                self.state.notify_error(error.to_string());
                return;
            }
        };
        // Nothing is known to be at the adapter yet.
        self.state.breakpoints_mut().forget_sent();
        let launch = self.launch_arguments(config, &program);
        feeder.text(b"\n[starting the debugger]\n");
        self.debug.session = Some(Active {
            client,
            config: config.id().clone(),
            launch: Some(launch),
            feeder,
            initialized: false,
            configured: false,
            thread: None,
            waiting: vec![Waiting {
                at: Instant::now(),
                kind: Kind::Initialize(initialize),
            }],
            ending: None,
        });
        let data = self.state.debug_mut();
        data.forget_stop();
        data.forget_watch_values();
        data.status = Status::Starting;
        self.needs_redraw = true;
    }

    /// The arguments of `launch` for `lldb-dap`: the program, how it is run, and that the client
    /// starts it in a terminal.
    fn launch_arguments(&self, config: &RunConfig, program: &std::path::Path) -> Value {
        let root = self.project_root();
        let cwd = match config.cwd() {
            Some(folder) => root.join(folder),
            None => root,
        };
        let mut env: Vec<String> = match config.what() {
            What::Cargo(target) => run_env(target)
                .into_iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect(),
            _ => Vec::new(),
        };
        env.extend(
            config
                .env()
                .iter()
                .map(|(name, value)| format!("{name}={value}")),
        );
        json!({
            "program": program,
            "args": config.args(),
            "cwd": cwd,
            "env": env,
            "stopOnEntry": false,
            "runInTerminal": true,
        })
    }

    /// What the adapter did, or answered: looked at when the app is told of it.
    pub(super) fn debug_changed(&mut self) {
        let Some(mut active) = self.debug.session.take() else {
            return;
        };
        while let Some(update) = active.client.next_update() {
            self.debug_update(&mut active, update);
        }
        self.debug_answers(&mut active);
        self.debug_finish_setup(&mut active);
        self.needs_redraw = true;
        match active.ending.take() {
            Some(message) => self.end_debug(active, &message),
            None => self.debug.session = Some(active),
        }
    }

    fn debug_update(&mut self, active: &mut Active, update: Update) {
        match update {
            Update::Event(event) => self.debug_event(active, event),
            Update::Reverse {
                seq,
                request: ReverseRequest::RunInTerminal(run),
            } => self.debug_run_in_terminal(active, seq, run),
            Update::Closed(reason) => {
                active
                    .ending
                    .get_or_insert(format!("the debug adapter went away: {reason}"));
            }
        }
    }

    fn debug_event(&mut self, active: &mut Active, event: DapEvent) {
        match event {
            DapEvent::Initialized => {
                active.initialized = true;
                self.debug_send_breakpoints(active);
            }
            DapEvent::Stopped(stop) => self.debug_stopped(active, stop),
            DapEvent::Continued(_) => {
                let data = self.state.debug_mut();
                data.forget_stop();
                data.status = Status::Running;
            }
            DapEvent::Exited { exit_code } => {
                let how = match exit_code {
                    0 => "the program ended".to_owned(),
                    code => format!("the program ended with code {code}"),
                };
                self.state.notify(how);
            }
            DapEvent::Terminated => {
                active
                    .ending
                    .get_or_insert("the debug session is over".to_owned());
            }
            _ => {}
        }
    }

    // --- the program is started in a tab ---

    fn debug_run_in_terminal(
        &mut self,
        active: &mut Active,
        seq: i64,
        run: RunInTerminalArguments,
    ) {
        let outcome = self.start_debuggee_in_tab(active, &run);
        let _ = active.client.respond_run_in_terminal(seq, outcome);
    }

    /// Starts what the adapter asked for in the tab of the run configuration. The process
    /// (which waits for the debugger) is the one the adapter attaches to.
    fn start_debuggee_in_tab(
        &mut self,
        active: &Active,
        run: &RunInTerminalArguments,
    ) -> Result<RunInTerminalResponse, String> {
        let Some((program, args)) = run.args.split_first() else {
            return Err("nothing to start".to_owned());
        };
        let Some(index) = self.state.terminal().run_tab(active.config.as_str()) else {
            return Err("the terminal tab of the run is gone".to_owned());
        };
        let Some(events) = self.background.sender() else {
            return Err("the event loop is not running".to_owned());
        };
        let (rows, columns) = self.terminal_dimensions();
        let directory = match run.cwd.as_str() {
            "" => self.project_root(),
            cwd => PathBuf::from(cwd),
        };
        let env = run
            .env
            .iter()
            .filter_map(|(name, value)| Some((name.clone(), value.clone()?)))
            .collect();
        let spawn = SpawnConfig {
            shell: Shell {
                program: program.clone(),
                args: args.to_vec(),
            },
            directory,
            env,
            rows,
            columns,
        };
        let session = Session::spawn(&spawn, super::run::tab_wake(&events))
            .map_err(|error| error.to_string())?;
        let process_id = session.process_id().map(i64::from);
        let title = self
            .runs_config_name(&active.config)
            .unwrap_or_else(|| "debug".to_owned());
        self.state
            .terminal_mut()
            .replace_session(index, session, title);
        Ok(RunInTerminalResponse {
            process_id,
            shell_process_id: None,
        })
    }

    // --- the breakpoints ---

    /// Hands the adapter the breakpoints of every file that has changed since it was last told.
    fn debug_send_breakpoints(&mut self, active: &mut Active) {
        for (path, lines) in self.state.breakpoints_mut().take_changes() {
            let arguments = SetBreakpointsArguments {
                source: Source::file(&path.to_string_lossy()),
                breakpoints: lines
                    .iter()
                    .map(|line| SourceBreakpoint::at(i64::try_from(line + 1).unwrap_or(i64::MAX)))
                    .collect(),
            };
            match active.client.set_breakpoints(&arguments) {
                Ok(reply) => active.waiting.push(Waiting {
                    at: Instant::now(),
                    kind: Kind::SetBreakpoints { path, reply },
                }),
                Err(error) => {
                    active.ending.get_or_insert(error.to_string());
                    return;
                }
            }
        }
    }

    /// Called after every input and event while a session goes: breakpoints set or moved
    /// since are handed over.
    pub(super) fn debug_sync_breakpoints(&mut self) {
        let Some(mut active) = self.debug.session.take() else {
            return;
        };
        if active.initialized {
            self.debug_send_breakpoints(&mut active);
        }
        match active.ending.take() {
            Some(message) => self.end_debug(active, &message),
            None => self.debug.session = Some(active),
        }
    }

    /// The setup ends when the adapter has the breakpoints: `configurationDone`.
    fn debug_finish_setup(&mut self, active: &mut Active) {
        let still_setting = active
            .waiting
            .iter()
            .any(|waiting| matches!(waiting.kind, Kind::SetBreakpoints { .. }));
        if !active.initialized || active.configured || still_setting || active.ending.is_some() {
            return;
        }
        match active.client.configuration_done() {
            Ok(reply) => {
                active.configured = true;
                active.waiting.push(Waiting {
                    at: Instant::now(),
                    kind: Kind::ConfigurationDone(reply),
                });
                self.state.debug_mut().status = Status::Running;
            }
            Err(error) => {
                active.ending.get_or_insert(error.to_string());
            }
        }
    }

    // --- what comes back ---

    fn debug_answers(&mut self, active: &mut Active) {
        let mut still = Vec::new();
        for waiting in std::mem::take(&mut active.waiting) {
            let Waiting { at, kind } = waiting;
            let kind = match kind {
                Kind::Initialize(mut reply) => match reply.try_take() {
                    None => Kind::Initialize(reply),
                    Some(result) => {
                        self.debug_initialized(active, result);
                        continue;
                    }
                },
                Kind::Launch(mut reply) => match reply.try_take() {
                    None => Kind::Launch(reply),
                    Some(Ok(_)) => continue,
                    Some(Err(error)) => {
                        active
                            .ending
                            .get_or_insert(format!("the program could not be launched: {error}"));
                        continue;
                    }
                },
                Kind::SetBreakpoints { path, mut reply } => match reply.try_take() {
                    None => Kind::SetBreakpoints { path, reply },
                    Some(result) => {
                        self.debug_breakpoints_answered(&path, result);
                        continue;
                    }
                },
                Kind::ConfigurationDone(mut reply) => match reply.try_take() {
                    None => Kind::ConfigurationDone(reply),
                    Some(Ok(_)) => continue,
                    Some(Err(error)) => {
                        active.ending.get_or_insert(error.to_string());
                        continue;
                    }
                },
                Kind::Threads(mut reply) => match reply.try_take() {
                    None => Kind::Threads(reply),
                    Some(result) => {
                        self.debug_threads(active, result);
                        continue;
                    }
                },
                Kind::Stack(mut reply) => match reply.try_take() {
                    None => Kind::Stack(reply),
                    Some(result) => {
                        self.debug_stack(active, result);
                        continue;
                    }
                },
                Kind::Scopes { frame, mut reply } => match reply.try_take() {
                    None => Kind::Scopes { frame, reply },
                    Some(result) => {
                        self.debug_scopes(active, frame, result);
                        continue;
                    }
                },
                Kind::Variables {
                    row,
                    start,
                    target,
                    mut reply,
                } => match reply.try_take() {
                    None => Kind::Variables {
                        row,
                        start,
                        target,
                        reply,
                    },
                    Some(result) => {
                        match target {
                            Target::Pane => self.debug_variables(row, start, result),
                            Target::Hover => self.debug_hover_variables(active, row, start, result),
                            Target::Watch => self.debug_watch_variables(active, row, start, result),
                        }
                        continue;
                    }
                },
                Kind::Watch {
                    expression,
                    mut reply,
                } => match reply.try_take() {
                    None => Kind::Watch { expression, reply },
                    Some(result) => {
                        self.debug_watch_evaluated(active, &expression, result);
                        continue;
                    }
                },
                Kind::Evaluate { ticket, mut reply } => match reply.try_take() {
                    None => Kind::Evaluate { ticket, reply },
                    Some(result) => {
                        self.debug_evaluated(active, ticket, result);
                        continue;
                    }
                },
                Kind::Continue(mut reply) => match reply.try_take() {
                    None => Kind::Continue(reply),
                    Some(result) => {
                        if let Err(error) = result {
                            self.state.notify_error(error.to_string());
                        }
                        continue;
                    }
                },
                Kind::Step(name, mut reply) => match reply.try_take() {
                    None => Kind::Step(name, reply),
                    Some(result) => {
                        if let Err(error) = result {
                            self.state.notify_error(error.to_string());
                        }
                        continue;
                    }
                },
                Kind::Disconnect(mut reply) => match reply.try_take() {
                    None => Kind::Disconnect(reply),
                    Some(_) => {
                        active
                            .ending
                            .get_or_insert("the debug session was stopped".to_owned());
                        continue;
                    }
                },
            };
            still.push(Waiting { at, kind });
        }
        // What the answers asked for was put in `waiting` meanwhile.
        active.waiting.extend(still);
    }

    fn debug_initialized(&mut self, active: &mut Active, result: Result<Capabilities, DapError>) {
        if let Err(error) = result {
            active
                .ending
                .get_or_insert(format!("the debug adapter did not start: {error}"));
            return;
        }
        let Some(arguments) = active.launch.take() else {
            return;
        };
        // The answer to `launch` comes when the setup is done (for some adapters), so it is not
        // waited for here.
        match active.client.launch(arguments) {
            Ok(reply) => active.waiting.push(Waiting {
                at: Instant::now(),
                kind: Kind::Launch(reply),
            }),
            Err(error) => {
                active.ending.get_or_insert(error.to_string());
            }
        }
    }

    fn debug_breakpoints_answered(
        &mut self,
        path: &std::path::Path,
        result: Result<SetBreakpointsResponse, DapError>,
    ) {
        match result {
            Ok(response) => {
                let answers: Vec<Answer> = response
                    .breakpoints
                    .iter()
                    .map(|breakpoint| Answer {
                        verified: breakpoint.verified,
                        message: breakpoint.message.clone(),
                    })
                    .collect();
                self.state.breakpoints_mut().answered(path, &answers);
            }
            Err(error) => self
                .state
                .notify_error(format!("the debugger could not set breakpoints: {error}")),
        }
    }

    // --- stopped ---

    fn debug_stopped(&mut self, active: &mut Active, stop: Stopped) {
        let data = self.state.debug_mut();
        data.forget_stop();
        data.status = Status::Stopped {
            reason: stop.reason.clone(),
            description: stop.description.clone(),
        };
        let thread = stop.thread_id.or(active.thread);
        match thread {
            Some(thread) => {
                active.thread = Some(thread);
                self.debug_ask_stack(active, thread);
            }
            None => match active.client.threads() {
                Ok(reply) => active.waiting.push(Waiting {
                    at: Instant::now(),
                    kind: Kind::Threads(reply),
                }),
                Err(error) => {
                    active.ending.get_or_insert(error.to_string());
                }
            },
        }
    }

    fn debug_threads(&mut self, active: &mut Active, result: Result<ThreadsResponse, DapError>) {
        let first = result
            .ok()
            .and_then(|response| response.threads.first().map(|t| t.id));
        if let Some(thread) = first {
            active.thread = Some(thread);
            self.debug_ask_stack(active, thread);
        }
    }

    fn debug_ask_stack(&mut self, active: &mut Active, thread: i64) {
        let arguments = StackTraceArguments {
            thread_id: thread,
            start_frame: None,
            levels: Some(FRAMES),
        };
        match active.client.stack_trace(&arguments) {
            Ok(reply) => active.waiting.push(Waiting {
                at: Instant::now(),
                kind: Kind::Stack(reply),
            }),
            Err(error) => {
                active.ending.get_or_insert(error.to_string());
            }
        }
    }

    fn debug_stack(&mut self, active: &mut Active, result: Result<StackTraceResponse, DapError>) {
        let response = match result {
            Ok(response) => response,
            Err(error) => {
                self.state
                    .notify_error(format!("the stack could not be read: {error}"));
                return;
            }
        };
        let frames: Vec<Frame> = response
            .stack_frames
            .iter()
            .map(|frame| Frame {
                id: frame.id,
                name: frame.name.clone(),
                path: frame
                    .source
                    .as_ref()
                    .and_then(|source| source.path.as_deref())
                    .map(PathBuf::from),
                line: usize::try_from(frame.line.saturating_sub(1)).unwrap_or(0),
                column: usize::try_from(frame.column.saturating_sub(1)).unwrap_or(0),
            })
            .collect();
        let data = self.state.debug_mut();
        data.frames = frames;
        data.selected_frame = 0;
        let first = data.frames.first().map(|frame| frame.id);
        self.debug_announce_stop();
        self.debug_reveal_frame(true);
        // The pane with the stack and the variables comes to the front (not the keyboard).
        self.state.show_debug();
        // A layout file from before the Debug view has none: the stop would show nothing.
        if !self.state.layout().contains(&ComponentKind::Debug)
            && !std::mem::replace(&mut self.debug.told_no_view, true)
        {
            self.state.notify(
                "the layout has no Debug view: add Debug to the views of the bottom bar in layout.ron (views: [Terminal, Errors, Debug])",
            );
        }
        if let Some(frame) = first {
            self.debug_ask_scopes(active, frame);
        }
        self.debug_hover_refresh(active);
        self.debug_watches_refresh(active);
    }

    /// Says where the program stopped, unless it did what was asked: a step, or a breakpoint
    /// (the arrow and the pane show those).
    fn debug_announce_stop(&mut self) {
        let data = self.state.debug();
        let Status::Stopped { reason, .. } = &data.status else {
            return;
        };
        if matches!(reason.as_str(), "step" | "breakpoint") {
            return;
        }
        let place = data.frame().map(|frame| {
            let file = frame
                .path
                .as_ref()
                .and_then(|path| path.file_name())
                .map_or_else(String::new, |name| {
                    format!(" {}:{}", name.to_string_lossy(), frame.line + 1)
                });
            format!("{}{file}", frame.name)
        });
        let message = match place {
            Some(place) => format!("stopped ({reason}) in {place}"),
            None => format!("stopped ({reason})"),
        };
        self.state.notify(message);
    }

    /// Shows the file and line of the chosen frame: opens the file if it is not open, and puts
    /// the cursor there. A frame with no file of the project (a library) shows nothing.
    pub(super) fn debug_reveal_frame(&mut self, keyboard_to_editor: bool) {
        let Some((path, line)) = self
            .state
            .debug()
            .frame()
            .and_then(|frame| Some((frame.path.clone()?, frame.line)))
        else {
            return;
        };
        if !path.is_file() {
            return;
        }
        if self.state.workspace_mut().open_path(&path).is_err() {
            return;
        }
        self.state.edit(|editor| {
            let at = crate::buffer::Position::new(line, 0);
            let at = editor.buffer().clamp_position(at);
            editor.go_to(at);
        });
        // The keys that step are the editor's: after a stop the keyboard is there, not in the
        // terminal tab the program runs in.
        if keyboard_to_editor {
            self.state.set_focus(super::state::Focus::Editor);
        }
    }

    fn debug_ask_scopes(&mut self, active: &mut Active, frame: i64) {
        match active.client.scopes(frame) {
            Ok(reply) => active.waiting.push(Waiting {
                at: Instant::now(),
                kind: Kind::Scopes { frame, reply },
            }),
            Err(error) => {
                active.ending.get_or_insert(error.to_string());
            }
        }
    }

    fn debug_scopes(
        &mut self,
        active: &mut Active,
        frame: i64,
        result: Result<ScopesResponse, DapError>,
    ) {
        // An answer for a frame that is not the chosen one any more is not wanted.
        if self.state.debug().frame().map(|f| f.id) != Some(frame) {
            return;
        }
        let Ok(response) = result else {
            return;
        };
        let scopes: Vec<(String, i64)> = response
            .scopes
            .iter()
            .map(|scope| (scope.name.clone(), scope.variables_reference))
            .collect();
        let tree = &mut self.state.debug_mut().tree;
        tree.set_scopes(&scopes);
        // The first scope (the locals) is opened: it is what is looked at first.
        if let Some(read) = tree.toggle(0) {
            self.debug_ask_variables(active, read);
        }
    }

    /// Asks for the children a row of the tree opened for.
    fn debug_ask_variables(&mut self, active: &mut Active, read: Read) {
        let arguments = match read.page {
            Some((start, count)) => VariablesArguments::page(read.reference, start, count),
            None => VariablesArguments::of(read.reference),
        };
        match active.client.variables(&arguments) {
            Ok(reply) => active.waiting.push(Waiting {
                at: Instant::now(),
                kind: Kind::Variables {
                    row: read.row,
                    start: read.page.map_or(0, |(start, _)| start),
                    target: Target::Pane,
                    reply,
                },
            }),
            Err(_) => self.state.debug_mut().tree.failed(read.row),
        }
    }

    fn debug_variables(
        &mut self,
        row: u64,
        start: i64,
        result: Result<VariablesResponse, DapError>,
    ) {
        let tree = &mut self.state.debug_mut().tree;
        match result {
            Ok(response) => {
                let children = response
                    .variables
                    .into_iter()
                    .map(|variable| Incoming {
                        name: variable.name,
                        value: variable.value,
                        type_name: variable.type_name,
                        reference: variable.variables_reference,
                        total: variable.indexed_variables.or(variable.named_variables),
                    })
                    .collect();
                tree.fill(row, start, children);
            }
            Err(_) => tree.failed(row),
        }
    }

    /// Chooses frame `index` of the stack (in the pane): the editor shows where it is, and the
    /// variables are those of the frame.
    pub(super) fn debug_select_frame(&mut self, index: usize) {
        let data = self.state.debug_mut();
        let Some(frame) = data.frames.get(index).map(|frame| frame.id) else {
            return;
        };
        data.selected_frame = index;
        data.tree.clear();
        self.state.debug_view_mut().select_frame(index);
        self.debug_reveal_frame(false);
        if let Some(mut active) = self.debug.session.take() {
            self.debug_ask_scopes(&mut active, frame);
            self.debug_hover_refresh(&mut active);
            self.debug_watches_refresh(&mut active);
            self.debug.session = Some(active);
        }
    }

    /// Opens or closes the row `index` of the variables.
    pub(super) fn debug_toggle_variable(&mut self, index: usize) {
        let Some(read) = self.state.debug_mut().tree.toggle(index) else {
            return;
        };
        if let Some(mut active) = self.debug.session.take() {
            self.debug_ask_variables(&mut active, read);
            self.debug.session = Some(active);
        } else {
            // The session is over: what was shown cannot be opened.
            self.state.debug_mut().tree.failed(read.row);
        }
    }

    // --- the controls ---

    /// Goes on from where the program stopped.
    pub(super) fn debug_continue(&mut self) {
        self.debug_resume(Control::Continue);
    }

    pub(super) fn debug_step_over(&mut self) {
        self.debug_resume(Control::Over);
    }

    pub(super) fn debug_step_into(&mut self) {
        self.debug_resume(Control::Into);
    }

    pub(super) fn debug_step_out(&mut self) {
        self.debug_resume(Control::Out);
    }

    fn debug_resume(&mut self, control: Control) {
        let Some(mut active) = self.debug.session.take() else {
            self.state.notify("there is no debug session");
            return;
        };
        let stopped = matches!(self.state.debug().status, Status::Stopped { .. });
        let thread = active.thread.filter(|_| stopped);
        let Some(thread) = thread else {
            let why = match self.state.debug().status {
                Status::Starting => "the debugger is starting",
                _ => "the program is running: it can be stepped when it stops",
            };
            self.state.notify(why);
            self.debug.session = Some(active);
            return;
        };
        let sent = match control {
            Control::Continue => active.client.continue_(thread).map(Kind::Continue),
            Control::Over => active
                .client
                .next(thread)
                .map(|reply| Kind::Step("next", reply)),
            Control::Into => active
                .client
                .step_in(thread)
                .map(|reply| Kind::Step("stepIn", reply)),
            Control::Out => active
                .client
                .step_out(thread)
                .map(|reply| Kind::Step("stepOut", reply)),
        };
        match sent {
            Ok(kind) => {
                active.waiting.push(Waiting {
                    at: Instant::now(),
                    kind,
                });
                let data = self.state.debug_mut();
                data.forget_stop();
                data.status = Status::Running;
                self.debug.session = Some(active);
            }
            Err(error) => self.end_debug(active, &error.to_string()),
        }
        self.needs_redraw = true;
    }

    /// Ends the session: the program is stopped with it.
    pub(super) fn debug_stop(&mut self) {
        // A stop asked for while a restart goes on is a stop.
        self.debug.restart = false;
        self.stop_session();
    }

    /// Ends the session and starts it again when it is over (a start when none goes).
    pub(super) fn debug_restart(&mut self) {
        if !self.debug_active() {
            self.debug_start();
            return;
        }
        if self.debug.restart {
            return;
        }
        self.debug.restart = true;
        self.stop_session();
    }

    fn stop_session(&mut self) {
        let Some(mut active) = self.debug.session.take() else {
            self.state.notify("there is no debug session");
            return;
        };
        match active.client.disconnect(true) {
            Ok(reply) => {
                active.waiting.push(Waiting {
                    at: Instant::now(),
                    kind: Kind::Disconnect(reply),
                });
                self.debug.session = Some(active);
            }
            Err(_) => self.end_debug(active, "the debug session was stopped"),
        }
        self.needs_redraw = true;
    }

    /// The session is over: the adapter goes (the program with it), what was shown is cleared
    /// and the breakpoints will be handed over again to the next one.
    fn end_debug(&mut self, active: Active, message: &str) {
        // A tab that never got its program (the adapter failed first) shows the build, and says
        // that it is over; one that got its program has another session, and this is nothing to it.
        active.feeder.text(format!("\n[{message}]\n").as_bytes());
        active.feeder.finish(1);
        drop(active);
        self.state.breakpoints_mut().forget_sent();
        self.close_debug_hover();
        let data = self.state.debug_mut();
        data.forget_stop();
        data.forget_watch_values();
        data.status = Status::Ended(message.to_owned());
        self.state.notify(message.to_owned());
        self.needs_redraw = true;
        if std::mem::take(&mut self.debug.restart) {
            self.debug_start();
        }
    }

    /// Gives up on a request that was never answered.
    pub(super) fn debug_forget_stale(&mut self) {
        let Some(active) = &self.debug.session else {
            return;
        };
        let late = active
            .waiting
            .iter()
            .find(|waiting| waiting.at.elapsed() > waiting.kind.limit());
        let Some(late) = late else {
            return;
        };
        let message = match late.kind {
            Kind::Disconnect(_) => "the debug session was stopped".to_owned(),
            _ => format!("the debug adapter did not answer {}", late.kind.name()),
        };
        if let Some(active) = self.debug.session.take() {
            self.end_debug(active, &message);
        }
    }

    /// The name of the run configuration `id`, for the title of its tab.
    fn runs_config_name(&self, id: &ConfigId) -> Option<String> {
        self.runs
            .list
            .as_ref()
            .and_then(|list| list.get(id))
            .map(|config| config.name().to_owned())
    }

    /// Debugs with `adapter` instead of starting `lldb-dap`, for tests.
    #[cfg(test)]
    pub(crate) fn with_debug_adapter(mut self, adapter: Adapter) -> Self {
        self.debug.adapter = Some(adapter);
        self
    }
}

#[derive(Debug, Clone, Copy)]
enum Control {
    Continue,
    Over,
    Into,
    Out,
}

/// Starts the adapter as a program: `lldb-dap`, or what `TRCODE_DEBUG_ADAPTER` names.
fn default_adapter() -> Adapter {
    Arc::new(|wake| {
        let program =
            std::env::var("TRCODE_DEBUG_ADAPTER").unwrap_or_else(|_| "lldb-dap".to_owned());
        Client::spawn(&program, &[], wake)
    })
}

/// What to tell the user when the adapter could not be started.
fn adapter_trouble(error: &DapError) -> String {
    match error {
        DapError::Spawn { program, source } if source.kind() == std::io::ErrorKind::NotFound => {
            format!(
                "the debug adapter `{program}` is not installed or not on the PATH (lldb-dap comes with LLVM; TRCODE_DEBUG_ADAPTER names another)"
            )
        }
        other => other.to_string(),
    }
}
