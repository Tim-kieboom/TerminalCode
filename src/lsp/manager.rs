//! Runs the servers: starts them on threads of their own so that nothing waits
//! for a handshake, takes what they send, and carries out what the
//! [`Supervisor`] decides.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

use super::Incoming;
use super::config::ServerKey;
use super::launch::{LaunchSpec, Launcher, Session, StartError};
use super::supervisor::{Effect, Notice, Supervisor};

type Wake = Arc<dyn Fn() + Send + Sync>;
type Started = (ServerKey, Result<Session, StartError>);

pub(crate) struct Manager {
    supervisor: Supervisor,
    launcher: Arc<dyn Launcher>,
    sessions: HashMap<ServerKey, Session>,
    started: Receiver<Started>,
    starter: Sender<Started>,
    /// Called when something is waiting for [`Manager::poll`].
    wake: Wake,
}

impl Manager {
    pub(crate) fn new(
        launcher: Arc<dyn Launcher>,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let (starter, started) = mpsc::channel();
        Self {
            supervisor: Supervisor::default(),
            launcher,
            sessions: HashMap::new(),
            started,
            starter,
            wake: Arc::new(wake),
        }
    }

    /// A file wants the server `key`; starts it if it was not asked for before.
    pub(crate) fn ensure(&mut self, key: &ServerKey, spec: LaunchSpec) -> Vec<Notice> {
        let effects = self.supervisor.ensure(key, spec);
        self.carry_out(effects)
    }

    /// Takes everything that is waiting: servers that finished starting, what
    /// the running ones said, and connections that ended. Returns what the
    /// user is to be told.
    pub(crate) fn poll(&mut self) -> Vec<Notice> {
        let mut notices = Vec::new();
        while let Ok((key, result)) = self.started.try_recv() {
            if !self.supervisor.is_starting(&key) {
                // Started for a server that was reset since: nobody wants it, and
                // dropping the session ends the program.
                continue;
            }
            let effects = match result {
                Ok(session) => {
                    self.sessions.insert(key.clone(), session);
                    self.supervisor.launched(&key, Ok(()))
                }
                Err(error) => self.supervisor.launched(&key, Err(error)),
            };
            notices.extend(self.carry_out(effects));
        }

        let keys: Vec<ServerKey> = self.sessions.keys().cloned().collect();
        for key in keys {
            notices.extend(self.take_incoming(&key));
        }
        notices
    }

    fn take_incoming(&mut self, key: &ServerKey) -> Vec<Notice> {
        let mut notices = Vec::new();
        while let Some(incoming) = self
            .sessions
            .get(key)
            .and_then(|session| session.incoming.try_recv().ok())
        {
            match incoming {
                Incoming::Notification { method, params } => {
                    self.supervisor.notification(key, &method, &params);
                }
                Incoming::Closed(reason) => {
                    // Dropping the session ends the program if it is still there.
                    self.sessions.remove(key);
                    let effects = self.supervisor.closed(key, &reason, Instant::now());
                    notices.extend(self.carry_out(effects));
                    break;
                }
            }
        }
        notices
    }

    fn carry_out(&mut self, effects: Vec<Effect>) -> Vec<Notice> {
        let mut notices = Vec::new();
        for effect in effects {
            match effect {
                Effect::Notify(notice) => notices.push(notice),
                Effect::Launch(key, spec) => self.launch(key, spec),
            }
        }
        notices
    }

    fn launch(&self, key: ServerKey, spec: LaunchSpec) {
        let launcher = Arc::clone(&self.launcher);
        let starter = self.starter.clone();
        let wake = Arc::clone(&self.wake);
        std::thread::spawn(move || {
            let result = launcher.launch(&spec, Arc::clone(&wake));
            let _ = starter.send((key, result));
            wake();
        });
    }

    /// What the status bar says about the server `key`.
    pub(crate) fn status(&self, key: &ServerKey) -> Option<String> {
        self.supervisor.status(key)
    }

    /// Ends every server and forgets them, so that the next file that wants
    /// one starts it again.
    pub(crate) fn reset(&mut self) {
        self.sessions.clear();
        self.supervisor.reset();
        // A start that was under way finishes into nothing.
        while self.started.try_recv().is_ok() {}
    }
}
