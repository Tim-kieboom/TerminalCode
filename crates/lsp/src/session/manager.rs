//! Runs the servers: starts them on threads of their own so that nothing waits
//! for a handshake, takes what they send, and carries out what the
//! [`Supervisor`] decides.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

use crate::Incoming;
use crate::config::ServerKey;
use crate::launch::{LaunchSpec, Launcher, Session, StartError};
use crate::supervisor::{Effect, Notice, Supervisor};

type Wake = Arc<dyn Fn() + Send + Sync>;
type Started = (ServerKey, Result<Session, StartError>);

pub struct Manager {
    supervisor: Supervisor,
    launcher: Arc<dyn Launcher>,
    sessions: HashMap<ServerKey, Session>,
    /// How many sessions each server has had, so that a server that started again
    /// is told from the one it replaces.
    generations: HashMap<ServerKey, u64>,
    started: Receiver<Started>,
    starter: Sender<Started>,
    /// Called when something is waiting for [`Manager::poll`].
    wake: Wake,
    /// What running servers said that is for the app to handle, taken by
    /// [`Manager::take_notifications`].
    notifications: Vec<ServerNotification>,
}

/// A notification from a server that the app is to look at.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerNotification {
    pub key: ServerKey,
    pub method: String,
    pub params: serde_json::Value,
}

impl Manager {
    pub fn new(launcher: Arc<dyn Launcher>, wake: impl Fn() + Send + Sync + 'static) -> Self {
        let (starter, started) = mpsc::channel();
        Self {
            supervisor: Supervisor::default(),
            launcher,
            sessions: HashMap::new(),
            generations: HashMap::new(),
            started,
            starter,
            wake: Arc::new(wake),
            notifications: Vec::new(),
        }
    }

    /// A file wants the server `key`; starts it if it was not asked for before.
    pub fn ensure(&mut self, key: &ServerKey, spec: LaunchSpec) -> Vec<Notice> {
        let effects = self.supervisor.ensure(key, spec);
        self.carry_out(effects)
    }

    /// Takes everything that is waiting: servers that finished starting, what
    /// the running ones said, and connections that ended. Returns what the
    /// user is to be told.
    pub fn poll(&mut self) -> Vec<Notice> {
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
                    *self.generations.entry(key.clone()).or_default() += 1;
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
                    if method != "$/progress" {
                        self.notifications.push(ServerNotification {
                            key: key.clone(),
                            method,
                            params,
                        });
                    }
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
    pub fn status(&self, key: &ServerKey) -> Option<String> {
        self.supervisor.status(key)
    }

    /// Ends every server and forgets them, so that the next file that wants
    /// one starts it again.
    pub fn reset(&mut self) {
        self.sessions.clear();
        self.supervisor.reset();
        // A start that was under way finishes into nothing.
        while self.started.try_recv().is_ok() {}
    }
}

impl Manager {
    /// The servers that are running now, with the number of the session each is in.
    pub fn running(&self) -> Vec<(ServerKey, u64)> {
        self.sessions
            .keys()
            .map(|key| (key.clone(), self.generations.get(key).copied().unwrap_or(0)))
            .collect()
    }

    /// Sends a notification to the server `key`. One that is not running, or
    /// whose connection broke, is left to [`Manager::poll`] to find out.
    pub fn notify(&self, key: &ServerKey, method: &str, params: serde_json::Value) {
        if let Some(session) = self.sessions.get(key) {
            let _ = session.connection.notify(method, params);
        }
    }
}

impl Manager {
    /// What the servers said since the last time, other than progress (which
    /// shows in the status by itself): diagnostics, messages.
    pub fn take_notifications(&mut self) -> Vec<ServerNotification> {
        std::mem::take(&mut self.notifications)
    }
}

impl Manager {
    /// What the server `key` offers for completion; `None` when it is not
    /// running or offers none.
    pub fn completion_provider(&self, key: &ServerKey) -> Option<crate::Provider> {
        self.sessions.get(key)?.server.completion()
    }

    /// Asks the server `key` what could be typed at `position` of the document
    /// `uri`. `None` when the server is not running or the request could not
    /// be sent; the answer is looked for with [`PendingCompletion::try_take`].
    pub fn request_completion(
        &self,
        key: &ServerKey,
        uri: &str,
        position: crate::position::LspPosition,
        trigger: crate::Trigger,
    ) -> Option<crate::PendingCompletion> {
        let session = self.sessions.get(key)?;
        crate::completion::request(&session.connection, uri, position, trigger).ok()
    }
}

impl Manager {
    /// Asks the server `key` for the edits that go with `item`. `None` when the
    /// server is not running or the request could not be sent.
    pub fn resolve_completion(
        &self,
        key: &ServerKey,
        item: &crate::CompletionItem,
    ) -> Option<crate::PendingResolve> {
        let session = self.sessions.get(key)?;
        crate::completion::resolve(&session.connection, item).ok()
    }
}

impl Manager {
    /// Whether the server `key` runs and answers hover requests.
    pub fn offers_hover(&self, key: &ServerKey) -> bool {
        self.sessions
            .get(key)
            .is_some_and(|session| crate::hover::offered(&session.server.capabilities))
    }

    /// Asks the server `key` what it can say about `position` of `uri`. `None`
    /// when it is not running or the request could not be sent.
    pub fn request_hover(
        &self,
        key: &ServerKey,
        uri: &str,
        position: crate::position::LspPosition,
    ) -> Option<crate::PendingHover> {
        let session = self.sessions.get(key)?;
        crate::hover::request(&session.connection, uri, position).ok()
    }
}

impl Manager {
    /// Whether the server `key` runs and answers definition requests.
    pub fn offers_definition(&self, key: &ServerKey) -> bool {
        self.sessions
            .get(key)
            .is_some_and(|session| crate::definition::offered(&session.server.capabilities))
    }

    /// Asks the server `key` where the thing at `position` of `uri` is defined.
    /// `None` when it is not running or the request could not be sent.
    pub fn request_definition(
        &self,
        key: &ServerKey,
        uri: &str,
        position: crate::position::LspPosition,
    ) -> Option<crate::PendingDefinition> {
        let session = self.sessions.get(key)?;
        crate::definition::request(&session.connection, uri, position).ok()
    }
}

impl Manager {
    /// Whether the server `key` runs and renames symbols.
    pub fn offers_rename(&self, key: &ServerKey) -> bool {
        self.sessions
            .get(key)
            .is_some_and(|session| crate::rename::offered(&session.server.capabilities))
    }

    /// Whether the server `key` also says whether a position can be renamed.
    pub fn offers_prepare_rename(&self, key: &ServerKey) -> bool {
        self.sessions
            .get(key)
            .is_some_and(|session| crate::rename::prepare_offered(&session.server.capabilities))
    }

    /// Asks the server `key` whether the thing at `position` of `uri` can be
    /// renamed. `None` when it is not running or the request could not be sent.
    pub fn request_prepare_rename(
        &self,
        key: &ServerKey,
        uri: &str,
        position: crate::position::LspPosition,
    ) -> Option<crate::PendingPrepare> {
        let session = self.sessions.get(key)?;
        crate::rename::request_prepare(&session.connection, uri, position).ok()
    }

    /// Asks the server `key` for the edits that rename the thing at `position`
    /// of `uri` to `new_name`.
    pub fn request_rename(
        &self,
        key: &ServerKey,
        uri: &str,
        position: crate::position::LspPosition,
        new_name: &str,
    ) -> Option<crate::PendingRename> {
        let session = self.sessions.get(key)?;
        crate::rename::request_rename(&session.connection, uri, position, new_name).ok()
    }
}

impl Manager {
    /// Whether the server `key` runs and lists code actions.
    pub fn offers_code_actions(&self, key: &ServerKey) -> bool {
        self.sessions
            .get(key)
            .is_some_and(|session| crate::code_action::offered(&session.server.capabilities))
    }

    /// Whether the server `key` completes code actions with `codeAction/resolve`.
    pub fn offers_code_action_resolve(&self, key: &ServerKey) -> bool {
        self.sessions.get(key).is_some_and(|session| {
            crate::code_action::resolve_offered(&session.server.capabilities)
        })
    }

    /// Whether the server `key` runs commands.
    pub fn offers_execute_command(&self, key: &ServerKey) -> bool {
        self.sessions.get(key).is_some_and(|session| {
            crate::code_action::execute_offered(&session.server.capabilities)
        })
    }

    /// Asks the server `key` what can be done at `start..end` of `uri`. `None`
    /// when it is not running or the request could not be sent.
    pub fn request_code_actions(
        &self,
        key: &ServerKey,
        uri: &str,
        start: crate::position::LspPosition,
        end: crate::position::LspPosition,
        diagnostics: &[crate::ContextDiagnostic],
    ) -> Option<crate::PendingActions> {
        let session = self.sessions.get(key)?;
        crate::code_action::request(&session.connection, uri, start, end, diagnostics).ok()
    }

    /// Asks the server `key` to complete `action`.
    pub fn request_resolve_code_action(
        &self,
        key: &ServerKey,
        action: &crate::CodeAction,
    ) -> Option<crate::PendingActionResolve> {
        let session = self.sessions.get(key)?;
        crate::code_action::request_resolve(&session.connection, action).ok()
    }

    /// Asks the server `key` to run `command`.
    pub fn request_execute_command(
        &self,
        key: &ServerKey,
        command: &crate::Command,
    ) -> Option<crate::PendingCommand> {
        let session = self.sessions.get(key)?;
        crate::code_action::request_execute(&session.connection, command).ok()
    }
}

impl Manager {
    /// Whether the server `key` runs (has finished starting).
    pub fn is_running(&self, key: &ServerKey) -> bool {
        self.sessions.contains_key(key)
    }

    /// Whether the server `key` is being started, and is not running yet.
    pub fn is_starting(&self, key: &ServerKey) -> bool {
        self.supervisor.is_starting(key) && !self.sessions.contains_key(key)
    }

    /// Whether the server `key` runs and formats documents.
    pub fn offers_formatting(&self, key: &ServerKey) -> bool {
        self.sessions
            .get(key)
            .is_some_and(|session| crate::formatting::offered(&session.server.capabilities))
    }

    /// Asks the server `key` to format `uri`. `None` when it is not running or the
    /// request could not be sent.
    pub fn request_formatting(
        &self,
        key: &ServerKey,
        uri: &str,
        tab_size: u32,
        insert_spaces: bool,
    ) -> Option<crate::PendingFormat> {
        let session = self.sessions.get(key)?;
        crate::formatting::request(&session.connection, uri, tab_size, insert_spaces).ok()
    }
}

impl Manager {
    /// Whether the server `key` runs and formats a range of a document.
    pub fn offers_range_formatting(&self, key: &ServerKey) -> bool {
        self.sessions
            .get(key)
            .is_some_and(|session| crate::formatting::range_offered(&session.server.capabilities))
    }

    /// Asks the server `key` to format `start..end` of `uri`. `None` when it is not
    /// running or the request could not be sent.
    pub fn request_range_formatting(
        &self,
        key: &ServerKey,
        uri: &str,
        start: crate::position::LspPosition,
        end: crate::position::LspPosition,
        tab_size: u32,
        insert_spaces: bool,
    ) -> Option<crate::PendingFormat> {
        let session = self.sessions.get(key)?;
        crate::formatting::request_range(
            &session.connection,
            uri,
            start,
            end,
            tab_size,
            insert_spaces,
        )
        .ok()
    }
}
