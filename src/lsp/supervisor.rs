//! The life of the servers: which are starting, running or off, and what to do
//! when one cannot start or dies. It only decides; the manager does the
//! starting. Time is passed in, so the policy can be tested without waiting.
//!
//! The policy (see "LSP client" in `docs/design.md`): a broken server never
//! blocks typing, and the user is told once.
//! - A server that cannot start, or exits at once, is reported and is not
//!   tried again until the settings change or the project is reopened.
//! - One that dies while running is restarted, at most [`MAX_RESTARTS`] times
//!   within [`RESTART_WINDOW`], with a notice each time; then it stays off.

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::config::ServerKey;
use super::launch::{LaunchSpec, StartError};
use super::progress::Progress;

/// How many times a server that keeps dying is started again.
pub(crate) const MAX_RESTARTS: usize = 3;
/// How far back a death counts towards [`MAX_RESTARTS`].
pub(crate) const RESTART_WINDOW: Duration = Duration::from_secs(60);

/// Something to tell the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Notice {
    /// An error stays on screen until it is dismissed; the rest go by.
    pub(crate) error: bool,
    pub(crate) text: String,
}

impl Notice {
    fn info(text: String) -> Self {
        Self { error: false, text }
    }

    fn error(text: String) -> Self {
        Self { error: true, text }
    }
}

/// What the supervisor wants done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Effect {
    Launch(ServerKey, LaunchSpec),
    Notify(Notice),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    /// Being started; the handshake has not finished.
    Starting,
    Running,
    /// Not running and not to be started again by itself.
    Off,
}

#[derive(Debug)]
struct Slot {
    spec: LaunchSpec,
    state: State,
    /// When it died while running, within the window.
    deaths: VecDeque<Instant>,
    progress: Progress,
}

#[derive(Debug, Default)]
pub(crate) struct Supervisor {
    slots: BTreeMap<ServerKey, Slot>,
}

impl Supervisor {
    /// A file wants the server `key`. Starts it unless it was already asked
    /// for; a server that is off stays off.
    pub(crate) fn ensure(&mut self, key: &ServerKey, spec: LaunchSpec) -> Vec<Effect> {
        if self.slots.contains_key(key) {
            return Vec::new();
        }
        self.slots.insert(
            key.clone(),
            Slot {
                spec: spec.clone(),
                state: State::Starting,
                deaths: VecDeque::new(),
                progress: Progress::default(),
            },
        );
        vec![Effect::Launch(key.clone(), spec)]
    }

    /// A start finished: the server is running, or why it is not.
    pub(crate) fn launched(
        &mut self,
        key: &ServerKey,
        result: Result<(), StartError>,
    ) -> Vec<Effect> {
        let Some(slot) = self.slots.get_mut(key) else {
            return Vec::new();
        };
        if slot.state != State::Starting {
            return Vec::new();
        }
        match result {
            Ok(()) => {
                slot.state = State::Running;
                slot.progress = Progress::default();
                Vec::new()
            }
            Err(error) => {
                slot.state = State::Off;
                let text = format!(
                    "{} could not start: {}; it is not tried again until the settings change or the project is reopened",
                    slot.spec.command,
                    error.reason()
                );
                vec![Effect::Notify(Notice::error(text))]
            }
        }
    }

    /// A running server's connection ended, at `now`, for `reason`.
    pub(crate) fn closed(&mut self, key: &ServerKey, reason: &str, now: Instant) -> Vec<Effect> {
        let Some(slot) = self.slots.get_mut(key) else {
            return Vec::new();
        };
        if slot.state != State::Running {
            return Vec::new();
        }
        slot.progress = Progress::default();
        while slot
            .deaths
            .front()
            .is_some_and(|died| now.saturating_duration_since(*died) > RESTART_WINDOW)
        {
            slot.deaths.pop_front();
        }

        let command = slot.spec.command.clone();
        if slot.deaths.len() >= MAX_RESTARTS {
            slot.state = State::Off;
            let text = format!(
                "{command} stopped again ({reason}) after being restarted {MAX_RESTARTS} times within {} s; it stays off until the settings change or the project is reopened",
                RESTART_WINDOW.as_secs()
            );
            return vec![Effect::Notify(Notice::error(text))];
        }
        slot.deaths.push_back(now);
        slot.state = State::Starting;
        let text = format!(
            "{command} stopped ({reason}); restarting it ({} of {MAX_RESTARTS})",
            slot.deaths.len()
        );
        vec![
            Effect::Notify(Notice::info(text)),
            Effect::Launch(key.clone(), slot.spec.clone()),
        ]
    }

    /// A notification from a running server.
    pub(crate) fn notification(&mut self, key: &ServerKey, method: &str, params: &Value) {
        let Some(slot) = self.slots.get_mut(key) else {
            return;
        };
        if method == "$/progress" {
            slot.progress.update(params);
        }
    }

    /// Forgets every server, so the next file that wants one starts it afresh:
    /// for when the settings changed, or the project was reopened.
    pub(crate) fn reset(&mut self) {
        self.slots.clear();
    }

    /// What the status bar says about the server `key`, or nothing when it
    /// was never asked for.
    pub(crate) fn status(&self, key: &ServerKey) -> Option<String> {
        let slot = self.slots.get(key)?;
        let name = &slot.spec.command;
        Some(match slot.state {
            State::Starting => format!("{name}: starting"),
            State::Running => match slot.progress.text() {
                Some(job) => format!("{name}: {job}"),
                None => name.clone(),
            },
            State::Off => format!("{name}: off"),
        })
    }

    pub(crate) fn is_running(&self, key: &ServerKey) -> bool {
        self.slots
            .get(key)
            .is_some_and(|slot| slot.state == State::Running)
    }
}

impl Supervisor {
    /// Whether a start of `key` is under way, so that its result is wanted. A
    /// start that was under way when everything was reset is not.
    pub(crate) fn is_starting(&self, key: &ServerKey) -> bool {
        self.slots
            .get(key)
            .is_some_and(|slot| slot.state == State::Starting)
    }
}
