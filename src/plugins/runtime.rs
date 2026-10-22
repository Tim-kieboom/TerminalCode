//! The threads plugins run on: they own every instance, so a slow or hung plugin
//! never holds up the editor, and tell the app what happened through a callback.
//! Most plugins share one thread; a plugin whose manifest says `thread = "own"` gets a
//! thread of its own, so what it does that takes long stalls only itself (and it is
//! the only kind of plugin that gets a budget that grows with what it is sent). Each
//! thread also holds the latest state of the git repository, which the app hands it,
//! so a plugin's questions about git are answered with no trip to the UI. The threads
//! reach one another through [`Routes`], which is how a plugin calls another.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Arc, Mutex, Weak};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use super::calls::{Calls, Outgoing};
use super::exec::{self, Control, Sink};
use super::git_view::Sections;
use super::instance::{Compiled, Delivered, Failure, Instance};
use super::policy::{Strikes, Verdict};
use super::runs::Runs;
use super::view::node_of;
use super::{CALLBACK_FUEL, FUEL_PER_BYTE, OWNED_FUEL_CEILING, Thread};
use crate::git::Status;
use crate::plugin_api::{Capability, FromPlugin, Refusal, RunPattern, RunStream, ToPlugin};
use crate::ui::view::ViewNode;

/// A change to the repository a plugin asked for, which the app carries out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WriteOp {
    Stage(String),
    Unstage(String),
    StageAll,
    UnstageAll,
    Commit(String),
    Fetch,
    Pull,
    Push,
    Cancel,
}

/// What the runtime tells the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Notice {
    /// A plugin replaced what its pane shows.
    View { plugin: Box<str>, content: ViewNode },
    /// A plugin trapped and was started again, fresh.
    Restarted { plugin: Box<str>, reason: String },
    /// A plugin trapped too often and is off.
    Disabled { plugin: Box<str>, reason: String },
    /// A plugin cannot run at all.
    Broken { plugin: Box<str>, reason: String },
    /// A plugin said something that was not understood (told once for each start).
    Rejected { plugin: Box<str>, reason: String },
    /// A plugin asked to run a command in a folder the user does not trust (yet): the
    /// app asks the user, and tells the runtime ([`Runtime::set_workspace`]).
    NeedsTrust { plugin: Box<str> },
    /// A plugin with the capability asked to open a file. The app opens it and
    /// answers the plugin with [`Runtime::send`].
    OpenFile {
        plugin: Box<str>,
        request: u64,
        path: String,
    },
    /// A plugin with the capability asked to change the repository. The app does
    /// it and answers the plugin with [`Runtime::send`].
    GitWrite {
        plugin: Box<str>,
        request: u64,
        op: WriteOp,
    },
}

enum Command {
    Load {
        plugin: Box<str>,
        wasm: Vec<u8>,
        grants: HashSet<Capability>,
    },
    Send {
        plugin: Box<str>,
        message: ToPlugin,
    },
    /// The plugin is turned off.
    Unload {
        plugin: Box<str>,
    },
    /// The repository has a new state (`None`: there is none any more).
    Git(Option<Arc<Sections>>),
}

type Notify = Arc<dyn Fn(Notice) + Send + Sync>;

/// What the app last handed over about git: not yet (`None`), or the state (which may
/// be that there is none).
type GitNews = Option<Option<Arc<Sections>>>;

/// How a message gets to a plugin, whichever thread it is on, and which calls between
/// plugins are open. The threads hold it weakly: the [`Runtime`] ending ends them.
struct Routes {
    /// The shared thread; `None` once the runtime ends.
    shared: Mutex<Option<mpsc::Sender<Command>>>,
    /// The plugins that have a thread of their own, by name.
    owned: Mutex<HashMap<Box<str>, mpsc::Sender<Command>>>,
    calls: Mutex<Calls>,
    /// What plugins that run commands may run, and what runs.
    runs: Mutex<Runs>,
}

impl Routes {
    fn new(shared: mpsc::Sender<Command>) -> Self {
        Self {
            shared: Mutex::new(Some(shared)),
            owned: Mutex::new(HashMap::new()),
            calls: Mutex::new(Calls::default()),
            runs: Mutex::new(Runs::default()),
        }
    }

    /// Sends `message` to `plugin`, on the thread it is on.
    fn send(&self, plugin: &str, message: ToPlugin) {
        let command = Command::Send {
            plugin: plugin.into(),
            message,
        };
        let owned = self.owned.lock().expect("the lock is not poisoned");
        match owned.get(plugin) {
            Some(commands) => {
                let _ = commands.send(command);
            }
            None => {
                drop(owned);
                self.command(command);
            }
        }
    }

    fn command(&self, command: Command) {
        if let Some(commands) = &*self.shared.lock().expect("the lock is not poisoned") {
            // The thread only ends when the runtime does.
            let _ = commands.send(command);
        }
    }

    fn close(&self) {
        *self.shared.lock().expect("the lock is not poisoned") = None;
        self.owned.lock().expect("the lock is not poisoned").clear();
    }

    fn deliver(&self, outgoing: impl IntoIterator<Item = Outgoing>) {
        for Outgoing { to, message } in outgoing {
            self.send(&to, message);
        }
    }

    fn calls(&self) -> std::sync::MutexGuard<'_, Calls> {
        self.calls.lock().expect("the lock is not poisoned")
    }

    /// `plugin` may be called.
    fn serve(&self, plugin: &str) {
        self.calls().serve(plugin);
    }

    /// `plugin` is gone: its calls end, and the plugins on the other side are told.
    fn remove(&self, plugin: &str) {
        let told = self.calls().remove(plugin);
        self.deliver(told);
        self.runs().forget(plugin);
    }

    /// `plugin` starts again from nothing: its calls end, and the others are told.
    fn reset(&self, plugin: &str) {
        let told = self.calls().reset(plugin);
        self.deliver(told);
        self.runs().lose(plugin);
    }

    /// Opens a call of `caller`; the callee is sent it, or the caller is told no.
    fn call(&self, caller: &str, request: u64, to: &str, method: String, payload: String) {
        let opened = self.calls().open(caller, request, to, method, payload);
        match opened {
            Ok(outgoing) => self.deliver([outgoing]),
            Err(reason) => self.send(caller, ToPlugin::Refused { request, reason }),
        }
    }

    fn reply(&self, from: &str, to: &str, request: u64, payload: String, last: bool) {
        let routed = self.calls().reply(from, to, request, payload, last);
        self.deliver(routed);
    }

    fn cancel(&self, caller: &str, request: u64) {
        let routed = self.calls().cancel(caller, request);
        self.deliver(routed);
    }

    fn runs(&self) -> std::sync::MutexGuard<'_, Runs> {
        self.runs.lock().expect("the lock is not poisoned")
    }

    /// Runs a command for `plugin` if it may: the output and the end come to it as
    /// messages. The answer is why not, when not.
    fn run(
        self: &Arc<Self>,
        plugin: &str,
        request: u64,
        program: String,
        args: Vec<String>,
        cwd: &str,
    ) -> Result<(), Refusal> {
        let spec = self.runs().check(plugin, request, program, args, cwd)?;
        let control = Arc::new(Control::default());
        if !self.runs().begin(plugin, request, Arc::clone(&control)) {
            return Err(Refusal::Denied {
                capability: Capability::ProcessRun,
            });
        }
        let sink = Arc::new(PluginSink {
            routes: Arc::downgrade(self),
            plugin: plugin.into(),
            request,
        });
        exec::start(spec, sink, control).map_err(|error| {
            self.runs().finish(plugin, request);
            Refusal::Failed {
                detail: error.to_string(),
            }
        })
    }

    fn cancel_run(&self, plugin: &str, request: u64) {
        self.runs().cancel(plugin, request);
    }

    /// The folder commands run in, and whether the user trusts it; the plugins that run
    /// commands are told when that is not what it was.
    fn set_workspace(&self, root: PathBuf, trusted: bool) {
        let told = self.runs().set_workspace(root, trusted);
        for plugin in told {
            self.send(&plugin, ToPlugin::Trust { trusted });
        }
    }

    fn trusted(&self) -> bool {
        self.runs().trusted()
    }

    /// A call could not be delivered: its caller is told.
    fn abandon(&self, caller: &str, request: u64) {
        let told = self.calls().abandon(caller, request);
        self.deliver(told);
    }
}

/// Where what a command says goes: to the plugin that started it.
struct PluginSink {
    routes: Weak<Routes>,
    plugin: Box<str>,
    request: u64,
}

impl Sink for PluginSink {
    fn output(&self, stream: RunStream, text: String) {
        if let Some(routes) = self.routes.upgrade() {
            let message = ToPlugin::RunOutput {
                request: self.request,
                stream,
                text,
            };
            routes.send(&self.plugin, message);
        }
    }

    fn done(&self, code: Option<i32>, cancelled: bool, truncated: bool) {
        let Some(routes) = self.routes.upgrade() else {
            return;
        };
        routes.runs().finish(&self.plugin, self.request);
        let message = ToPlugin::RunDone {
            request: self.request,
            code,
            cancelled,
            truncated,
        };
        routes.send(&self.plugin, message);
    }
}

/// A handle on the plugin threads. Dropping it ends the shared thread (and tells the
/// others to end: one that is in a long call ends when the call does).
pub(crate) struct Runtime {
    routes: Arc<Routes>,
    thread: Option<JoinHandle<()>>,
    notify: Notify,
    /// The last state of git handed over, for a thread that starts later.
    git: Mutex<GitNews>,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Runtime").finish_non_exhaustive()
    }
}

impl Runtime {
    /// Starts the shared thread; `notify` is called on it (and on the threads of
    /// plugins that have their own), for everything that happens.
    pub(crate) fn spawn(notify: impl Fn(Notice) + Send + Sync + 'static) -> Self {
        let notify: Notify = Arc::new(notify);
        let (commands, received) = mpsc::channel();
        let routes = Arc::new(Routes::new(commands));
        let host = Host::new(Arc::clone(&notify), false, None, Arc::downgrade(&routes));
        let thread = thread::Builder::new()
            .name("plugins".into())
            .spawn(move || host.run(&received))
            .expect("the operating system starts a thread");
        Self {
            routes,
            thread: Some(thread),
            notify,
            git: Mutex::new(None),
        }
    }

    /// Loads the module `wasm` as `plugin` (in place of one of that name), with
    /// the capabilities its manifest lists, and starts it.
    pub(crate) fn load(
        &self,
        plugin: &str,
        wasm: Vec<u8>,
        grants: impl IntoIterator<Item = Capability>,
    ) {
        self.load_on(plugin, wasm, grants, Thread::Shared);
    }

    /// Like [`Runtime::load`], on the thread the manifest asks for: the shared one, or
    /// a thread of the plugin's own.
    pub(crate) fn load_on(
        &self,
        plugin: &str,
        wasm: Vec<u8>,
        grants: impl IntoIterator<Item = Capability>,
        thread: Thread,
    ) {
        self.load_with(plugin, wasm, grants, thread, Vec::new());
    }

    /// Like [`Runtime::load_on`], for a plugin that may run the commands `run` allows (if
    /// it has `process.run`).
    pub(crate) fn load_with(
        &self,
        plugin: &str,
        wasm: Vec<u8>,
        grants: impl IntoIterator<Item = Capability>,
        thread: Thread,
        run: Vec<RunPattern>,
    ) {
        let grants: HashSet<Capability> = grants.into_iter().collect();
        let serves = grants.contains(&Capability::PluginServe);
        let runs = grants.contains(&Capability::ProcessRun);
        let load = Command::Load {
            plugin: plugin.into(),
            wasm,
            grants,
        };
        // Whatever the plugin was doing as the one that is replaced is over.
        self.routes.remove(plugin);
        // Wherever it was before (the manifest may have changed), it is not there now.
        let mut owned = self.routes.owned.lock().expect("the lock is not poisoned");
        owned.remove(plugin);
        // A call is sent through the lock on `owned`, so it waits until the plugin has
        // its load in front of it, and it cannot arrive on the thread it left.
        if serves {
            self.routes.serve(plugin);
        }
        if runs {
            self.routes.runs().allow(plugin, run);
        }
        match thread {
            Thread::Shared => self.routes.command(load),
            Thread::Own => {
                self.routes.command(Command::Unload {
                    plugin: plugin.into(),
                });
                let commands = self.spawn_owned(plugin);
                let _ = commands.send(load);
                owned.insert(plugin.into(), commands);
            }
        }
    }

    /// The thread of a plugin of its own, knowing the state of git as it is now.
    fn spawn_owned(&self, plugin: &str) -> mpsc::Sender<Command> {
        let (commands, received) = mpsc::channel();
        let git = self.git.lock().expect("the lock is not poisoned").clone();
        let host = Host::new(
            Arc::clone(&self.notify),
            true,
            git,
            Arc::downgrade(&self.routes),
        );
        thread::Builder::new()
            .name(format!("plugin {plugin}"))
            .spawn(move || host.run(&received))
            .expect("the operating system starts a thread");
        commands
    }

    /// Stops `plugin` and forgets it.
    pub(crate) fn unload(&self, plugin: &str) {
        self.routes.remove(plugin);
        let had_thread = self
            .routes
            .owned
            .lock()
            .expect("the lock is not poisoned")
            .remove(plugin)
            .is_some();
        // The thread of its own ends when its sender is gone; the shared one is told.
        if !had_thread {
            self.routes.command(Command::Unload {
                plugin: plugin.into(),
            });
        }
    }

    /// The folder commands run in (the project's) and whether the user trusts it. A plugin
    /// that may run commands is told, when it starts and when this changes.
    pub(crate) fn set_workspace(&self, root: PathBuf, trusted: bool) {
        self.routes.set_workspace(root, trusted);
    }

    /// Sends `message` to `plugin`.
    pub(crate) fn send(&self, plugin: &str, message: ToPlugin) {
        self.routes.send(plugin, message);
    }

    /// Hands over the state of the repository (generation `generation`), or
    /// says there is none. Plugins that may read it are told.
    pub(crate) fn publish_git(&self, state: Option<(u64, &Status)>) {
        let sections = state.map(|(generation, status)| Arc::new(Sections::of(generation, status)));
        *self.git.lock().expect("the lock is not poisoned") = Some(sections.clone());
        for commands in self
            .routes
            .owned
            .lock()
            .expect("the lock is not poisoned")
            .values()
        {
            let _ = commands.send(Command::Git(sections.clone()));
        }
        self.routes.command(Command::Git(sections));
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        // The threads of plugins of their own end when they are done with what they
        // are doing; waiting for a long parse to quit would be the editor hanging.
        self.routes.close();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A command a plugin asks to run.
struct Launch {
    request: u64,
    program: String,
    args: Vec<String>,
    cwd: String,
}

/// How a plugin stopped being what its calls were with.
#[derive(Debug, Clone, Copy)]
enum Lost {
    /// It trapped and starts again, fresh: it can still be called.
    Reset,
    /// It is off, replaced or broken.
    Gone,
}

/// A plugin that is loaded: its module, its instance while it has one, what it
/// may do, and how it has been doing.
struct Loaded {
    compiled: Compiled,
    instance: Option<Instance>,
    grants: HashSet<Capability>,
    strikes: Strikes,
    /// Whether what it said that was not understood was told since it started.
    rejection_told: bool,
    /// Whether it runs on a thread of its own, which decides its budget.
    owned: bool,
    /// How many bytes of text of a file it was last sent: what it holds, which what it
    /// does with every message after depends on (a preview draws it again).
    held: u64,
}

/// Everything the thread knows.
struct Host {
    plugins: HashMap<Box<str>, Loaded>,
    /// The latest state of the repository; `None` without one.
    git: Option<Arc<Sections>>,
    /// Whether this thread is the one of a single plugin that asked for its own.
    owned: bool,
    /// Whether the app has said anything about git yet; before that `git` being
    /// `None` means "not known yet", not "no repository".
    git_known: bool,
    /// Messages to deliver, which a plugin's questions and the state of git
    /// gave rise to. They wait their turn behind what the app sends, so a plugin
    /// that answers every answer with a question cannot keep the others waiting.
    queue: VecDeque<Command>,
    notify: Notify,
    /// How to reach the other plugins; gone when the runtime ended.
    routes: Weak<Routes>,
}

impl Host {
    fn new(notify: Notify, owned: bool, git: GitNews, routes: Weak<Routes>) -> Self {
        Self {
            plugins: HashMap::new(),
            git: git.clone().flatten(),
            owned,
            git_known: git.is_some(),
            queue: VecDeque::new(),
            notify,
            routes,
        }
    }

    fn run(mut self, commands: &mpsc::Receiver<Command>) {
        loop {
            let command = match commands.try_recv() {
                Ok(command) => command,
                Err(TryRecvError::Disconnected) => return,
                Err(TryRecvError::Empty) => match self.queue.pop_front() {
                    Some(command) => command,
                    None => match commands.recv() {
                        Ok(command) => command,
                        Err(_) => return,
                    },
                },
            };
            self.handle(command);
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Load {
                plugin,
                wasm,
                grants,
            } => self.load(plugin, &wasm, grants),
            Command::Send { plugin, message } => self.deliver(&plugin, message),
            Command::Unload { plugin } => {
                self.plugins.remove(&plugin);
            }
            Command::Git(sections) => {
                self.git = sections;
                self.git_known = true;
                let message = self.git_news();
                let readers: Vec<Box<str>> = self
                    .plugins
                    .iter()
                    .filter(|(_, loaded)| loaded.grants.contains(&Capability::GitRead))
                    .map(|(name, _)| name.clone())
                    .collect();
                for plugin in readers {
                    self.queue.push_back(Command::Send {
                        plugin,
                        message: message.clone(),
                    });
                }
            }
        }
    }

    fn load(&mut self, plugin: Box<str>, wasm: &[u8], grants: HashSet<Capability>) {
        self.plugins.remove(&plugin);
        match Compiled::new(wasm) {
            Ok(compiled) => {
                let loaded = Loaded {
                    compiled,
                    instance: None,
                    grants,
                    strikes: Strikes::default(),
                    rejection_told: false,
                    owned: self.owned,
                    held: 0,
                };
                self.plugins.insert(plugin.clone(), loaded);
                self.deliver(&plugin, ToPlugin::Start);
            }
            Err(failure) => {
                lost(&self.routes, &plugin, Lost::Gone);
                (self.notify)(Notice::Broken {
                    plugin,
                    reason: failure.reason().to_owned(),
                });
            }
        }
    }

    /// What plugins that may read git are told when its state changed.
    fn git_news(&self) -> ToPlugin {
        match &self.git {
            Some(sections) => ToPlugin::GitChanged {
                summary: sections.summary().clone(),
            },
            None => ToPlugin::GitUnavailable,
        }
    }

    /// Sends `message` to `plugin`, and starts it again, fresh, each time it traps,
    /// until it works or has trapped too often.
    fn deliver(&mut self, plugin: &str, message: ToPlugin) {
        let Some(loaded) = self.plugins.get_mut(plugin) else {
            // A call to a plugin that is not here (it failed to load, or went away as
            // the call was on its way) is told to its caller, not left open.
            if let (ToPlugin::Call { from, request, .. }, Some(routes)) =
                (&message, self.routes.upgrade())
            {
                routes.abandon(from, *request);
            }
            return;
        };

        // The text of a file goes only to a plugin that was allowed to have it.
        if matches!(
            message,
            ToPlugin::FileText { .. } | ToPlugin::FileCleared | ToPlugin::FileTooLarge { .. }
        ) && !loaded.grants.contains(&Capability::FilesText)
        {
            return;
        }

        let mut message = message;
        let name: Box<str> = plugin.into();
        loop {
            let starting = matches!(message, ToPlugin::Start);
            match call(loaded, message) {
                Ok(delivered) => {
                    let grants = loaded.grants.clone();
                    let requests = self.report(&name, delivered);
                    // A plugin that has just started (or started again) is told whether
                    // the folder is trusted; it knows nothing yet.
                    if starting
                        && grants.contains(&Capability::ProcessRun)
                        && let Some(routes) = self.routes.upgrade()
                    {
                        self.queue.push_front(Command::Send {
                            plugin: name.clone(),
                            message: ToPlugin::Trust {
                                trusted: routes.trusted(),
                            },
                        });
                    }
                    // A plugin that has just started (or started again) is told how
                    // git is; it knows nothing yet.
                    if starting && grants.contains(&Capability::GitRead) && self.git_known {
                        let news = self.git_news();
                        self.queue.push_front(Command::Send {
                            plugin: name.clone(),
                            message: news,
                        });
                    }
                    self.answer(&name, &grants, requests);
                    return;
                }
                Err(Failure::Broken(reason)) => {
                    self.plugins.remove(plugin);
                    lost(&self.routes, plugin, Lost::Gone);
                    (self.notify)(Notice::Broken {
                        plugin: name,
                        reason,
                    });
                    return;
                }
                Err(Failure::Trap(reason)) => {
                    loaded.instance = None;
                    match loaded.strikes.record(Instant::now()) {
                        Verdict::Restart => {
                            lost(&self.routes, plugin, Lost::Reset);
                            (self.notify)(Notice::Restarted {
                                plugin: name.clone(),
                                reason,
                            });
                            // A fresh instance knows nothing: it starts from the beginning.
                            message = ToPlugin::Start;
                            loaded.rejection_told = false;
                        }
                        Verdict::Disable => {
                            self.plugins.remove(plugin);
                            lost(&self.routes, plugin, Lost::Gone);
                            (self.notify)(Notice::Disabled {
                                plugin: name,
                                reason,
                            });
                            return;
                        }
                    }
                }
            }
        }
    }

    /// Tells the app what the plugin said about its pane, and gives back what
    /// it asked for that is not about its pane.
    fn report(&mut self, name: &str, delivered: Delivered) -> Vec<FromPlugin> {
        let mut requests = Vec::new();
        for message in delivered.messages {
            match message {
                FromPlugin::SetView { view } => (self.notify)(Notice::View {
                    plugin: name.into(),
                    content: node_of(view),
                }),
                // A message of a kind this version does not know (the plugin is
                // newer than the editor) is ignored.
                FromPlugin::Unknown => {}
                request => requests.push(request),
            }
        }
        if let Some(reason) = delivered.rejected.into_iter().next()
            && let Some(loaded) = self.plugins.get_mut(name)
            && !std::mem::replace(&mut loaded.rejection_told, true)
        {
            (self.notify)(Notice::Rejected {
                plugin: name.into(),
                reason,
            });
        }
        requests
    }

    /// Answers what a plugin asked for: pages of git from the state held here,
    /// changes to the repository by handing them to the app. Each is checked
    /// against what the plugin may do first.
    fn answer(&mut self, plugin: &str, grants: &HashSet<Capability>, requests: Vec<FromPlugin>) {
        for request in requests {
            if matches!(
                request,
                FromPlugin::Call { .. }
                    | FromPlugin::Reply { .. }
                    | FromPlugin::CallCancel { .. }
                    | FromPlugin::Run { .. }
                    | FromPlugin::RunCancel { .. }
            ) {
                self.route(plugin, grants, request);
                continue;
            }
            let (id, needs, needs_git) = match &request {
                FromPlugin::GitEntries { request, .. } => (*request, Capability::GitRead, true),
                FromPlugin::GitStage { request, .. }
                | FromPlugin::GitUnstage { request, .. }
                | FromPlugin::GitStageAll { request }
                | FromPlugin::GitUnstageAll { request }
                | FromPlugin::GitCommit { request, .. } => (*request, Capability::GitWrite, true),
                FromPlugin::GitFetch { request }
                | FromPlugin::GitPull { request }
                | FromPlugin::GitPush { request }
                | FromPlugin::GitCancel { request } => (*request, Capability::GitRemote, true),
                FromPlugin::OpenFile { request, .. } => (*request, Capability::EditorOpen, false),
                _ => continue,
            };
            let reply = if !grants.contains(&needs) {
                Some(refusal(id, Refusal::Denied { capability: needs }))
            } else if needs_git && self.git.is_none() {
                Some(refusal(id, Refusal::NoRepository))
            } else {
                self.carry_out(plugin, request)
            };
            if let Some(message) = reply {
                self.queue.push_back(Command::Send {
                    plugin: plugin.into(),
                    message,
                });
            }
        }
    }

    /// What a plugin says about a call: a call it makes (needs `plugin.call`), a part of
    /// an answer (needs `plugin.serve`), or giving a call up. A refusal goes back to the
    /// plugin; a part of an answer that has no open call to go to is dropped.
    fn route(&mut self, plugin: &str, grants: &HashSet<Capability>, message: FromPlugin) {
        let Some(routes) = self.routes.upgrade() else {
            return;
        };
        match message {
            FromPlugin::Call {
                request,
                to,
                method,
                payload,
            } => {
                if grants.contains(&Capability::PluginCall) {
                    routes.call(plugin, request, &to, method, payload);
                    return;
                }
                let denied = Refusal::Denied {
                    capability: Capability::PluginCall,
                };
                self.queue.push_back(Command::Send {
                    plugin: plugin.into(),
                    message: refusal(request, denied),
                });
            }
            FromPlugin::Reply {
                to,
                request,
                payload,
                last,
            } if grants.contains(&Capability::PluginServe) => {
                routes.reply(plugin, &to, request, payload, last);
            }
            FromPlugin::CallCancel { request } => routes.cancel(plugin, request),
            FromPlugin::Run {
                request,
                program,
                args,
                cwd,
            } => {
                let launch = Launch {
                    request,
                    program,
                    args,
                    cwd,
                };
                self.launch(&routes, plugin, grants, launch);
            }
            FromPlugin::RunCancel { request } if grants.contains(&Capability::ProcessRun) => {
                routes.cancel_run(plugin, request);
            }
            _ => {}
        }
    }

    /// A command a plugin asks to run (needs `process.run`): it runs, or the plugin is
    /// told why not. A folder that is not trusted is also something to ask the user.
    fn launch(
        &mut self,
        routes: &Arc<Routes>,
        plugin: &str,
        grants: &HashSet<Capability>,
        launch: Launch,
    ) {
        let Launch {
            request,
            program,
            args,
            cwd,
        } = launch;
        let outcome = match grants.contains(&Capability::ProcessRun) {
            true => routes.run(plugin, request, program, args, &cwd),
            false => Err(Refusal::Denied {
                capability: Capability::ProcessRun,
            }),
        };
        let Err(reason) = outcome else {
            return;
        };
        if reason == Refusal::Untrusted {
            (self.notify)(Notice::NeedsTrust {
                plugin: plugin.into(),
            });
        }
        self.queue.push_back(Command::Send {
            plugin: plugin.into(),
            message: refusal(request, reason),
        });
    }

    /// Does a request that is allowed and has a repository to do it in: the
    /// answer when it can be given here, nothing when the app will give it.
    fn carry_out(&self, plugin: &str, request: FromPlugin) -> Option<ToPlugin> {
        if let FromPlugin::OpenFile { request, path } = request {
            (self.notify)(Notice::OpenFile {
                plugin: plugin.into(),
                request,
                path,
            });
            return None;
        }
        let sections = self.git.as_ref()?;
        match request {
            FromPlugin::GitEntries {
                request,
                section,
                offset,
                limit,
            } => Some(sections.page(request, section, offset, limit)),
            FromPlugin::GitStage { request, path } => {
                self.to_app(plugin, request, WriteOp::Stage(path));
                None
            }
            FromPlugin::GitUnstage { request, path } => {
                self.to_app(plugin, request, WriteOp::Unstage(path));
                None
            }
            FromPlugin::GitStageAll { request } => {
                self.to_app(plugin, request, WriteOp::StageAll);
                None
            }
            FromPlugin::GitUnstageAll { request } => {
                self.to_app(plugin, request, WriteOp::UnstageAll);
                None
            }
            FromPlugin::GitCommit { request, message } => {
                self.to_app(plugin, request, WriteOp::Commit(message));
                None
            }
            FromPlugin::GitFetch { request } => {
                self.to_app(plugin, request, WriteOp::Fetch);
                None
            }
            FromPlugin::GitPull { request } => {
                self.to_app(plugin, request, WriteOp::Pull);
                None
            }
            FromPlugin::GitPush { request } => {
                self.to_app(plugin, request, WriteOp::Push);
                None
            }
            FromPlugin::GitCancel { request } => {
                self.to_app(plugin, request, WriteOp::Cancel);
                None
            }
            _ => None,
        }
    }

    fn to_app(&self, plugin: &str, request: u64, op: WriteOp) {
        (self.notify)(Notice::GitWrite {
            plugin: plugin.into(),
            request,
            op,
        });
    }
}

/// Tells the other plugins that `plugin` stopped being what their calls were with.
fn lost(routes: &Weak<Routes>, plugin: &str, how: Lost) {
    let Some(routes) = routes.upgrade() else {
        return;
    };
    match how {
        Lost::Reset => routes.reset(plugin),
        Lost::Gone => routes.remove(plugin),
    }
}

fn refusal(request: u64, reason: Refusal) -> ToPlugin {
    ToPlugin::Refused { request, reason }
}

/// One call, on the instance the plugin has or a fresh one.
fn call(loaded: &mut Loaded, message: ToPlugin) -> Result<Delivered, Failure> {
    let instance = match &mut loaded.instance {
        Some(instance) => instance,
        None => loaded.instance.insert(loaded.compiled.instantiate()?),
    };
    if let ToPlugin::FileText { text, .. } = &message {
        loaded.held = text.len() as u64;
    }
    let fuel = fuel_for(&message, loaded.owned, loaded.held);
    instance.deliver(message, fuel)
}

/// The budget of one message. On the shared thread it is the flat one, whatever the
/// message (more would stall the other plugins). A plugin on a thread of its own is
/// given more in proportion to what it is sent to work on, up to a ceiling: the text of
/// a file (and, for every message after it, as long as it holds it: a preview draws its
/// document again for a key or a new width), the payload of a call or of an answer to
/// one, and the text a command it started wrote. A language adapter parsing the listing
/// of a big project is the plugin this is for.
pub(crate) fn fuel_for(message: &ToPlugin, owned: bool, held: u64) -> u64 {
    if !owned {
        return CALLBACK_FUEL;
    }
    let bytes = match message {
        ToPlugin::FileText { text, .. } => text.len() as u64,
        ToPlugin::Call { payload, .. } | ToPlugin::Reply { payload, .. } => {
            (payload.len() as u64).max(held)
        }
        ToPlugin::RunOutput { text, .. } => (text.len() as u64).max(held),
        _ => held,
    };
    CALLBACK_FUEL
        .saturating_add(FUEL_PER_BYTE.saturating_mul(bytes))
        .min(OWNED_FUEL_CEILING)
}
