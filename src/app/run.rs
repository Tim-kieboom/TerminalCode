//! Run: the key that starts the active run configuration (see "Run configurations and a
//! debugger" in `docs/design.md`).
//!
//! Run goes through its steps one after the other, and waits where it has to: for the list of
//! configurations (worked out off the UI thread: `cargo metadata` runs a program), for the
//! answer to "stop it and run it again?", and for the saves that format first. Then every
//! modified file with a path is on disk, and:
//!
//! - a target of cargo is built by `cargo build` in a thread of its own, whose errors are written
//!   to the tab of the configuration (the tab is a session with no shell until the program
//!   starts), and the program it made is started in the same tab;
//! - a command of the file is started at once.
//!
//! There is one tab for each configuration, which a second Run reuses; the process it has is
//! stopped first, after the question.

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use tokio::sync::mpsc::Sender;

use super::App;
use super::state::Focus;
use crate::components::popups::confirm::Confirm;
use crate::components::terminal::{MAX_TABS, TabExit};
use crate::event::Event;
use crate::launch::{
    ArtifactFinder, ConfigId, LaunchFile, RunConfig, RunList, Targets, What, build_args, run_env,
};
use crate::pty::{Feeder, Session, Shell, SpawnConfig};

/// How often a running build is looked at, to see whether it is over or was given up.
const BUILD_POLL: Duration = Duration::from_millis(20);

/// How a build ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BuildOutcome {
    /// It worked, and this is the program it made.
    Built(PathBuf),
    /// Cargo ended with this code (or was stopped by a signal).
    Failed(Option<i32>),
    /// It worked but made no program (a target that is only checked, say).
    NoProgram,
    /// Cargo could not be started.
    NotStarted(String),
    /// The run was started again, or the tab closed, while it went on.
    Cancelled,
}

/// What Run is in the middle of.
#[derive(Debug)]
struct Waiting {
    /// The configuration, once it is chosen.
    config: Option<ConfigId>,
    step: Step,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// For the list of configurations.
    List,
    /// For the user's answer to "stop it and run it again?".
    Asking,
    /// For the saves that format first.
    Formatting,
}

/// A build that is going: which one (a run started again makes the one before it stale), how to
/// give it up, and where its errors go.
#[derive(Debug)]
struct Build {
    generation: u64,
    cancel: Arc<AtomicBool>,
    feeder: Feeder,
}

/// What Run keeps.
#[derive(Debug, Default)]
pub(super) struct Runs {
    /// The configurations, once they were worked out.
    list: Option<RunList>,
    loading: bool,
    /// What the Run key starts; the first that can be run until another is chosen.
    active: Option<ConfigId>,
    waiting: Option<Waiting>,
    builds: HashMap<ConfigId, Build>,
    generation: u64,
    /// The program that is cargo; tests use a stand-in.
    cargo: Option<String>,
}

impl App {
    /// The Run key.
    pub(super) fn run_active(&mut self) {
        if self.runs.waiting.is_some() {
            self.state.notify("a run is starting already");
            return;
        }
        self.runs.waiting = Some(Waiting {
            config: None,
            step: Step::List,
        });
        self.advance_run();
    }

    /// Runs the steps that can be taken, until one has to wait.
    fn advance_run(&mut self) {
        loop {
            let Some(waiting) = &self.runs.waiting else {
                return;
            };
            let moved = match (waiting.step, waiting.config.clone()) {
                (Step::List, _) => self.step_choose(),
                (Step::Asking, _) => return,
                (Step::Formatting, Some(id)) => self.step_after_saves(&id),
                (Step::Formatting, None) => false,
            };
            if !moved {
                return;
            }
        }
    }

    /// Called once a frame: a run that waits for the saves goes on when they are done.
    pub(super) fn run_step(&mut self) {
        if self
            .runs
            .waiting
            .as_ref()
            .is_some_and(|waiting| waiting.step == Step::Formatting)
        {
            self.advance_run();
        }
    }

    // --- the list ---

    /// Works the list out when there is none yet, then chooses the configuration, asks about a
    /// process that is still running in its tab, and saves. Returns whether the run can go on
    /// to the next step now.
    fn step_choose(&mut self) -> bool {
        if self.runs.list.is_none() {
            self.load_run_list();
            return false;
        }
        let Some(config) = self.chosen_config() else {
            self.runs.waiting = None;
            return false;
        };
        let id = config.id().clone();
        let name = config.name().to_owned();
        self.runs.active = Some(id.clone());
        if self.state.terminal().run_is_running(id.as_str()) {
            self.runs.waiting = Some(Waiting {
                config: Some(id.clone()),
                step: Step::Asking,
            });
            self.open_run_question(&name, &id);
            return false;
        }
        self.begin_saving(id)
    }

    /// Starts working out the list in a thread; its answer is [`Event::RunList`].
    fn load_run_list(&mut self) {
        if self.runs.loading {
            return;
        }
        let Some(events) = self.background.sender() else {
            self.runs.waiting = None;
            self.state
                .notify_error("running needs the event loop, which is not running");
            return;
        };
        self.runs.loading = true;
        let root = self.project_root();
        let cargo = self.cargo_program();
        thread::spawn(move || {
            let targets = Targets::fetch_with(&cargo, &root);
            let file = LaunchFile::load(&root);
            let list = RunList::build(targets.as_ref(), &file);
            let _ = events.blocking_send(Event::RunList(list));
        });
    }

    /// The list is worked out: what is wrong in the file is said, and Run goes on.
    pub(super) fn run_list_ready(&mut self, list: RunList) {
        if let Some(problem) = list.problems().first() {
            let more = list.problems().len() - 1;
            let and = match more {
                0 => String::new(),
                more => format!(" (and {more} more)"),
            };
            self.state
                .notify_error(format!(".trcode/launch.toml: {problem}{and}"));
        }
        self.runs.list = Some(list);
        self.runs.loading = false;
        self.advance_run();
    }

    /// The configuration to run: the active one, or the first that can be run. Says why when
    /// there is none.
    fn chosen_config(&mut self) -> Option<RunConfig> {
        let list = self.runs.list.as_ref()?;
        let active = self.runs.active.as_ref().and_then(|id| list.get(id));
        let found = match active {
            Some(config) => Some(config),
            None => list.iter().find(|config| config.is_available()),
        };
        let Some(config) = found else {
            let why = match list.note() {
                Some(note) => format!("nothing to run: {note}"),
                None => "nothing to run: the project has no binary or example, and \
                         .trcode/launch.toml has no command"
                    .to_owned(),
            };
            self.state.notify(why);
            return None;
        };
        if let Some(why) = config.unavailable() {
            let message = format!("{} cannot be run: {why}", config.name());
            self.state.notify(message);
            return None;
        }
        Some(config.clone())
    }

    // --- the question ---

    fn open_run_question(&mut self, name: &str, id: &ConfigId) {
        if !matches!(self.state.popup(), super::state::Popup::None) {
            // Something else is being asked: the run is dropped rather than queued behind it.
            self.runs.waiting = None;
            return;
        }
        self.state
            .open_confirm(Confirm::run_again(name, id.as_str()));
        self.needs_redraw = true;
    }

    /// The answer was yes: the process in the tab is stopped when the new one takes its place.
    pub(super) fn run_confirmed(&mut self, id: &str) {
        let waiting = self.runs.waiting.as_ref();
        if waiting.is_none_or(|waiting| waiting.step != Step::Asking) {
            return;
        }
        let id = ConfigId::from(id);
        if self.begin_saving(id) {
            self.advance_run();
        }
    }

    /// The answer was no, or something else: the run is dropped.
    pub(super) fn run_declined(&mut self) {
        self.runs.waiting = None;
    }

    // --- saving ---

    /// Saves every modified file that has a path. A file that cannot be saved stops the run.
    /// Returns whether the next step can be taken now.
    fn begin_saving(&mut self, id: ConfigId) -> bool {
        let report = self.save_all_reporting();
        if !report.failed.is_empty() || !report.changed_on_disk.is_empty() {
            self.runs.waiting = None;
            self.state
                .notify_error("not running: some files could not be saved");
            return false;
        }
        self.runs.waiting = Some(Waiting {
            config: Some(id),
            step: Step::Formatting,
        });
        true
    }

    /// Waits for the saves that format first; then starts.
    fn step_after_saves(&mut self, id: &ConfigId) -> bool {
        if self.formatting_pending() {
            return false;
        }
        self.runs.waiting = None;
        let config = self.runs.list.as_ref().and_then(|list| list.get(id));
        let Some(config) = config.cloned() else {
            return false;
        };
        self.start_run(&config);
        false
    }

    // --- starting ---

    fn start_run(&mut self, config: &RunConfig) {
        let Some(events) = self.background.sender() else {
            self.state
                .notify_error("running needs the event loop, which is not running");
            return;
        };
        let (rows, columns) = self.terminal_dimensions();
        match config.what() {
            What::Cargo(_) => {
                let wake = tab_wake(&events);
                let (session, feeder) = Session::detached(rows, columns, wake);
                if !self.place_run_tab(config, session) {
                    return;
                }
                self.start_build(config, feeder, events);
            }
            What::Command(program) => {
                let spawned = self.spawn_program(config, program, Vec::new(), rows, columns);
                match spawned {
                    Ok(session) => {
                        self.place_run_tab(config, session);
                    }
                    Err(error) => self.state.notify_error(error),
                }
            }
            What::Missing(_) => {}
        }
    }

    /// Puts `session` in the tab of the configuration: the tab it has, or a new one. Shows the
    /// pane and gives the tab the keyboard. Returns whether it could.
    fn place_run_tab(&mut self, config: &RunConfig, session: Session) -> bool {
        if !self.show_terminal_pane() {
            return false;
        }
        let id = config.id();
        // A build that was going in the tab is given up.
        if let Some(old) = self.runs.builds.remove(id) {
            old.cancel.store(true, Ordering::SeqCst);
        }
        match self.state.terminal().run_tab(id.as_str()) {
            Some(index) => {
                let title = config.name().to_owned();
                self.state
                    .terminal_mut()
                    .replace_session(index, session, title);
            }
            None => {
                if self.state.terminal().tab_count() >= MAX_TABS {
                    self.state
                        .notify_error(format!("at most {MAX_TABS} terminal tabs"));
                    return false;
                }
                let title = config.name().to_owned();
                self.state
                    .terminal_mut()
                    .start_run(session, title, id.to_string());
            }
        }
        self.state.set_focus(Focus::Terminal);
        self.needs_redraw = true;
        true
    }

    /// Starts `cargo build` for the target of `config` in a thread.
    fn start_build(&mut self, config: &RunConfig, feeder: Feeder, events: Sender<Event>) {
        let What::Cargo(target) = config.what() else {
            return;
        };
        self.runs.generation += 1;
        let generation = self.runs.generation;
        let cancel = Arc::new(AtomicBool::new(false));
        self.runs.builds.insert(
            config.id().clone(),
            Build {
                generation,
                cancel: Arc::clone(&cancel),
                feeder: feeder.clone(),
            },
        );
        let spec = BuildSpec {
            cargo: self.cargo_program(),
            args: build_args(target, config.release()),
            root: self.project_root(),
            wanted: target.id().clone(),
            cancel,
            feeder,
        };
        let id = config.id().to_string();
        thread::spawn(move || {
            let outcome = run_build(&spec);
            let _ = events.blocking_send(Event::RunBuilt {
                id,
                generation,
                outcome,
            });
        });
    }

    /// A build is over: the program starts in the tab, or the tab says what went wrong.
    pub(super) fn run_built(&mut self, id: &str, generation: u64, outcome: BuildOutcome) {
        let id = ConfigId::from(id);
        // The build of a run that was started again, or a tab that is gone, is not wanted.
        match self.runs.builds.get(&id) {
            Some(build) if build.generation == generation => {}
            _ => return,
        }
        let Some(build) = self.runs.builds.remove(&id) else {
            return;
        };
        let Some(config) = self.runs.list.as_ref().and_then(|list| list.get(&id)) else {
            return;
        };
        let config = config.clone();
        let (rows, columns) = self.terminal_dimensions();
        let Some(index) = self.state.terminal().run_tab(id.as_str()) else {
            return;
        };
        let said = match outcome {
            BuildOutcome::Cancelled => return,
            BuildOutcome::Built(program) => {
                let program = program.display().to_string();
                let spawned =
                    self.spawn_program(&config, &program, run_env_of(&config), rows, columns);
                match spawned {
                    Ok(session) => {
                        let title = config.name().to_owned();
                        self.state
                            .terminal_mut()
                            .replace_session(index, session, title);
                        self.needs_redraw = true;
                        return;
                    }
                    Err(error) => (error, 127),
                }
            }
            BuildOutcome::Failed(code) => (
                match code {
                    Some(code) => format!("[the build failed: cargo ended with code {code}]"),
                    None => "[the build failed: cargo was stopped]".to_owned(),
                },
                u32::try_from(code.unwrap_or(1)).unwrap_or(1),
            ),
            BuildOutcome::NoProgram => ("[the build made no program to run]".to_owned(), 1),
            BuildOutcome::NotStarted(why) => (format!("[could not start cargo: {why}]"), 127),
        };
        build.feeder.text(format!("\n{}\n", said.0).as_bytes());
        build.feeder.finish(said.1);
        self.needs_redraw = true;
    }

    /// Starts `program` for `config` in a session: with its arguments, the variables of the
    /// configuration (and `extra`), in its folder.
    fn spawn_program(
        &mut self,
        config: &RunConfig,
        program: &str,
        extra: Vec<(String, String)>,
        rows: u16,
        columns: u16,
    ) -> Result<Session, String> {
        let Some(events) = self.background.sender() else {
            return Err("running needs the event loop, which is not running".to_owned());
        };
        let root = self.project_root();
        let directory = match config.cwd() {
            Some(folder) => root.join(folder),
            None => root,
        };
        let mut env = extra;
        env.extend(config.env().iter().map(|(k, v)| (k.clone(), v.clone())));
        let spawn = SpawnConfig {
            shell: Shell {
                program: program.to_owned(),
                args: config.args().to_vec(),
            },
            directory,
            env,
            rows,
            columns,
        };
        Session::spawn(&spawn, tab_wake(&events)).map_err(|error| error.to_string())
    }

    /// The tab of a run ended: its screen says so (a program's exit; a build's was said), and
    /// so does a notification.
    pub(super) fn run_tab_ended(&mut self, exit: &TabExit) {
        let name = exit.title.as_str();
        let session = self.state.terminal().session_of(exit.number - 1);
        let is_program = session.is_some_and(|session| session.process_id().is_some());
        if !is_program {
            self.state.notify(format!("{name}: the build failed"));
            return;
        }
        if let Some(session) = session {
            session.show(&format!("\n[exited with code {}]\n", exit.code));
        }
        let how = match exit.code {
            0 => format!("{name} ended"),
            code => format!("{name} ended with code {code}"),
        };
        self.state.notify(how);
    }

    fn cargo_program(&self) -> String {
        self.runs
            .cargo
            .clone()
            .unwrap_or_else(|| "cargo".to_owned())
    }

    /// Runs `cargo` as this program, for tests.
    #[cfg(test)]
    pub(crate) fn with_cargo(mut self, program: &str) -> Self {
        self.runs.cargo = Some(program.to_owned());
        self
    }

    /// The configuration the Run key starts, for tests.
    #[cfg(test)]
    pub(crate) fn active_run(&self) -> Option<&ConfigId> {
        self.runs.active.as_ref()
    }
}

/// What the program of a target is told that `cargo run` would tell it.
fn run_env_of(config: &RunConfig) -> Vec<(String, String)> {
    match config.what() {
        What::Cargo(target) => run_env(target),
        _ => Vec::new(),
    }
}

/// The wake-up of a tab's session: delivery matters more than speed (a lost one leaves the
/// screen stale).
fn tab_wake(events: &Sender<Event>) -> impl Fn() + Send + Sync + 'static + use<> {
    let events = events.clone();
    move || {
        let _ = events.blocking_send(Event::TerminalChanged);
    }
}

/// What the thread that builds needs.
struct BuildSpec {
    cargo: String,
    args: Vec<String>,
    root: PathBuf,
    wanted: crate::launch::TargetId,
    cancel: Arc<AtomicBool>,
    feeder: Feeder,
}

/// Runs the build and tells how it went. Cargo's errors (its standard error) go to the tab as
/// they come; what it prints on its standard output is read for the program it made.
fn run_build(spec: &BuildSpec) -> BuildOutcome {
    let started = Command::new(&spec.cargo)
        .args(&spec.args)
        .current_dir(&spec.root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match started {
        Ok(child) => child,
        Err(error) => return BuildOutcome::NotStarted(error.to_string()),
    };
    let finder = child.stdout.take().map(|mut out| {
        let mut finder = ArtifactFinder::new(&spec.wanted);
        thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            while let Ok(read) = out.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                finder.feed(&buffer[..read]);
            }
            finder.finish()
        })
    });
    let errors = child.stderr.take().map(|mut err| {
        let feeder = spec.feeder.clone();
        thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            while let Ok(read) = err.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                feeder.text(&buffer[..read]);
            }
        })
    });
    let status = loop {
        if spec.cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => thread::sleep(BUILD_POLL),
            Err(_) => break None,
        }
    };
    let program = finder.and_then(|handle| handle.join().ok()).flatten();
    if let Some(handle) = errors {
        let _ = handle.join();
    }
    let Some(status) = status else {
        return BuildOutcome::Cancelled;
    };
    match (status.success(), program) {
        (true, Some(program)) => BuildOutcome::Built(program),
        (true, None) => BuildOutcome::NoProgram,
        (false, _) => BuildOutcome::Failed(status.code()),
    }
}
