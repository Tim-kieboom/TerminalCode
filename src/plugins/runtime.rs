//! The thread plugins run on: it owns every instance, so a slow or hung plugin
//! never holds up the editor, and tells the app what happened through a callback.

use std::collections::HashMap;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

use super::instance::{Compiled, Delivered, Failure, Instance};
use super::message::{FromPlugin, ToPlugin};
use super::policy::{Strikes, Verdict};
use crate::ui::view::ViewNode;

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
}

enum Command {
    Load { plugin: Box<str>, wasm: Vec<u8> },
    Send { plugin: Box<str>, message: ToPlugin },
}

/// A handle on the plugin thread. Dropping it ends the thread.
#[derive(Debug)]
pub(crate) struct Runtime {
    commands: Option<mpsc::Sender<Command>>,
    thread: Option<JoinHandle<()>>,
}

impl Runtime {
    /// Starts the thread; `notify` is called on it, for everything that happens.
    pub(crate) fn spawn(notify: impl Fn(Notice) + Send + 'static) -> Self {
        let (commands, received) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("plugins".into())
            .spawn(move || run(&received, &notify))
            .expect("the operating system starts a thread");
        Self {
            commands: Some(commands),
            thread: Some(thread),
        }
    }

    /// Loads the module `wasm` as `plugin` (in place of one of that name) and
    /// starts it.
    pub(crate) fn load(&self, plugin: &str, wasm: Vec<u8>) {
        self.command(Command::Load {
            plugin: plugin.into(),
            wasm,
        });
    }

    /// Tells `plugin` that something changed.
    pub(crate) fn refresh(&self, plugin: &str) {
        self.command(Command::Send {
            plugin: plugin.into(),
            message: ToPlugin::Refresh,
        });
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
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A plugin that is loaded: its module, its instance while it has one, and how
/// it has been doing.
struct Loaded {
    compiled: Compiled,
    instance: Option<Instance>,
    strikes: Strikes,
    /// Whether what it said that was not understood was told since it started.
    rejection_told: bool,
}

fn run(commands: &mpsc::Receiver<Command>, notify: &dyn Fn(Notice)) {
    let mut plugins: HashMap<Box<str>, Loaded> = HashMap::new();
    for command in commands {
        match command {
            Command::Load { plugin, wasm } => {
                plugins.remove(&plugin);
                match Compiled::new(&wasm) {
                    Ok(compiled) => {
                        let loaded = Loaded {
                            compiled,
                            instance: None,
                            strikes: Strikes::default(),
                            rejection_told: false,
                        };
                        plugins.insert(plugin.clone(), loaded);
                        deliver(&mut plugins, &plugin, ToPlugin::Start, notify);
                    }
                    Err(failure) => notify(Notice::Broken {
                        plugin,
                        reason: failure.reason().to_owned(),
                    }),
                }
            }
            Command::Send { plugin, message } => deliver(&mut plugins, &plugin, message, notify),
        }
    }
}

/// Sends `message` to `plugin`, and starts it again, fresh, each time it traps,
/// until it works or has trapped too often.
fn deliver(
    plugins: &mut HashMap<Box<str>, Loaded>,
    plugin: &str,
    message: ToPlugin,
    notify: &dyn Fn(Notice),
) {
    let Some(loaded) = plugins.get_mut(plugin) else {
        return;
    };
    let mut message = message;
    let name: Box<str> = plugin.into();
    loop {
        let outcome = call(loaded, message);
        match outcome {
            Ok(delivered) => {
                report(&name, loaded, delivered, notify);
                return;
            }
            Err(Failure::Broken(reason)) => {
                plugins.remove(plugin);
                notify(Notice::Broken {
                    plugin: name,
                    reason,
                });
                return;
            }
            Err(Failure::Trap(reason)) => {
                loaded.instance = None;
                match loaded.strikes.record(Instant::now()) {
                    Verdict::Restart => {
                        notify(Notice::Restarted {
                            plugin: name.clone(),
                            reason,
                        });
                        // A fresh instance knows nothing: it starts from the beginning.
                        message = ToPlugin::Start;
                        loaded.rejection_told = false;
                    }
                    Verdict::Disable => {
                        plugins.remove(plugin);
                        notify(Notice::Disabled {
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

/// One call, on the instance the plugin has or a fresh one.
fn call(loaded: &mut Loaded, message: ToPlugin) -> Result<Delivered, Failure> {
    let instance = match &mut loaded.instance {
        Some(instance) => instance,
        None => loaded.instance.insert(loaded.compiled.instantiate()?),
    };
    instance.deliver(message)
}

/// Tells the app what the plugin said.
fn report(name: &str, loaded: &mut Loaded, delivered: Delivered, notify: &dyn Fn(Notice)) {
    for message in delivered.messages {
        match message {
            FromPlugin::SetView { view } => notify(Notice::View {
                plugin: name.into(),
                content: view.into_node(),
            }),
            // A message of a kind this version does not know: the plugin is newer
            // than the editor, and what it says is ignored.
            FromPlugin::Unknown => {}
        }
    }
    if let Some(reason) = delivered.rejected.into_iter().next()
        && !std::mem::replace(&mut loaded.rejection_told, true)
    {
        notify(Notice::Rejected {
            plugin: name.into(),
            reason,
        });
    }
}
