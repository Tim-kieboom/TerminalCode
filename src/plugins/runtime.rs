//! The threads plugins run on: they own every instance, so a slow or hung plugin
//! never holds up the editor, and tell the app what happened through a callback.
//! Most plugins share one thread; a plugin whose manifest says `thread = "own"` gets a
//! thread of its own, so what it does that takes long stalls only itself (and it is
//! the only kind of plugin that gets a budget that grows with what it is sent). Each
//! thread also holds the latest state of the git repository, which the app hands it,
//! so a plugin's questions about git are answered with no trip to the UI.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use super::git_view::Sections;
use super::instance::{Compiled, Delivered, Failure, Instance};
use super::policy::{Strikes, Verdict};
use super::view::node_of;
use super::{CALLBACK_FUEL, FUEL_PER_BYTE, OWNED_FUEL_CEILING, Thread};
use crate::git::Status;
use crate::plugin_api::{Capability, FromPlugin, Refusal, ToPlugin};
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

/// A handle on the plugin threads. Dropping it ends the shared thread (and tells the
/// others to end: one that is in a long call ends when the call does).
pub(crate) struct Runtime {
    commands: Option<mpsc::Sender<Command>>,
    thread: Option<JoinHandle<()>>,
    notify: Notify,
    /// The plugins that have a thread of their own, by name.
    owned: Mutex<HashMap<Box<str>, mpsc::Sender<Command>>>,
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
        let host = Host::new(Arc::clone(&notify), false, None);
        let thread = thread::Builder::new()
            .name("plugins".into())
            .spawn(move || host.run(&received))
            .expect("the operating system starts a thread");
        Self {
            commands: Some(commands),
            thread: Some(thread),
            notify,
            owned: Mutex::new(HashMap::new()),
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
        let load = Command::Load {
            plugin: plugin.into(),
            wasm,
            grants: grants.into_iter().collect(),
        };
        // Wherever it was before (the manifest may have changed), it is not there now.
        let mut owned = self.owned.lock().expect("the lock is not poisoned");
        owned.remove(plugin);
        match thread {
            Thread::Shared => self.command(load),
            Thread::Own => {
                self.command(Command::Unload {
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
        let host = Host::new(Arc::clone(&self.notify), true, git);
        thread::Builder::new()
            .name(format!("plugin {plugin}"))
            .spawn(move || host.run(&received))
            .expect("the operating system starts a thread");
        commands
    }

    /// Stops `plugin` and forgets it.
    pub(crate) fn unload(&self, plugin: &str) {
        let had_thread = self
            .owned
            .lock()
            .expect("the lock is not poisoned")
            .remove(plugin)
            .is_some();
        // The thread of its own ends when its sender is gone; the shared one is told.
        if !had_thread {
            self.command(Command::Unload {
                plugin: plugin.into(),
            });
        }
    }

    /// Sends `message` to `plugin`.
    pub(crate) fn send(&self, plugin: &str, message: ToPlugin) {
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

    /// Hands over the state of the repository (generation `generation`), or
    /// says there is none. Plugins that may read it are told.
    pub(crate) fn publish_git(&self, state: Option<(u64, &Status)>) {
        let sections = state.map(|(generation, status)| Arc::new(Sections::of(generation, status)));
        *self.git.lock().expect("the lock is not poisoned") = Some(sections.clone());
        for commands in self
            .owned
            .lock()
            .expect("the lock is not poisoned")
            .values()
        {
            let _ = commands.send(Command::Git(sections.clone()));
        }
        self.command(Command::Git(sections));
    }

    fn command(&self, command: Command) {
        if let Some(commands) = &self.commands {
            // The thread only ends when this handle is dropped.
            let _ = commands.send(command);
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.commands = None;
        // The threads of plugins of their own end when they are done with what they
        // are doing; waiting for a long parse to quit would be the editor hanging.
        self.owned.lock().expect("the lock is not poisoned").clear();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
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
}

impl Host {
    fn new(notify: Notify, owned: bool, git: GitNews) -> Self {
        Self {
            plugins: HashMap::new(),
            git: git.clone().flatten(),
            owned,
            git_known: git.is_some(),
            queue: VecDeque::new(),
            notify,
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
                };
                self.plugins.insert(plugin.clone(), loaded);
                self.deliver(&plugin, ToPlugin::Start);
            }
            Err(failure) => (self.notify)(Notice::Broken {
                plugin,
                reason: failure.reason().to_owned(),
            }),
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

fn refusal(request: u64, reason: Refusal) -> ToPlugin {
    ToPlugin::Refused { request, reason }
}

/// One call, on the instance the plugin has or a fresh one.
fn call(loaded: &mut Loaded, message: ToPlugin) -> Result<Delivered, Failure> {
    let instance = match &mut loaded.instance {
        Some(instance) => instance,
        None => loaded.instance.insert(loaded.compiled.instantiate()?),
    };
    let fuel = fuel_for(&message, loaded.owned);
    instance.deliver(message, fuel)
}

/// The budget of one message. A plugin on a thread of its own is given more for the
/// text of a file, in proportion to its size, up to a ceiling; every other message,
/// and every message to a plugin on the shared thread, has the flat one (more there
/// would stall the other plugins).
pub(super) fn fuel_for(message: &ToPlugin, owned: bool) -> u64 {
    match message {
        ToPlugin::FileText { text, .. } if owned => {
            let wanted =
                CALLBACK_FUEL.saturating_add(FUEL_PER_BYTE.saturating_mul(text.len() as u64));
            wanted.min(OWNED_FUEL_CEILING)
        }
        _ => CALLBACK_FUEL,
    }
}
