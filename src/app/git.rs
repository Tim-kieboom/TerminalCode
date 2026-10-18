//! Keeping what git says up to date: a refresh runs on a thread of its own
//! when something may have changed (files changed outside the editor, the
//! window came back to the front), and its answer arrives as an event.

use std::time::{Duration, Instant};

use super::App;
use crate::event::Event;
use crate::git::{Git, GitError, Schedule, Status};

/// How the refreshes are going.
#[derive(Debug, Default)]
pub(super) struct Tracking {
    git: Git,
    schedule: Schedule,
    /// The top folder of the repository, once found.
    root: Option<std::path::PathBuf>,
    /// There is no repository or no `git`: nothing is asked any more.
    off: bool,
    /// A problem that comes back was already told, so it is told once.
    told: bool,
}

impl App {
    /// Says that what git shows may have changed: a refresh follows soon, and
    /// not before the one that is running ended.
    pub(crate) fn git_changed(&mut self) {
        if self.git.off || !self.background.is_connected() {
            return;
        }
        self.git.schedule.request(Instant::now());
    }

    /// When the next refresh may start, if one is wanted.
    pub(crate) fn git_deadline(&self) -> Option<Instant> {
        match self.git.off {
            true => None,
            false => self.git.schedule.deadline(),
        }
    }

    /// Starts the refresh that is wanted, if it may start now.
    pub(crate) fn git_due(&mut self) {
        if !self.git.schedule.is_due(Instant::now()) {
            return;
        }
        let Some(events) = self.background.sender() else {
            return;
        };
        let (git, project, root) = (
            self.git.git.clone(),
            self.project_root(),
            self.git.root.clone(),
        );
        self.git.schedule.started();
        std::thread::spawn(move || {
            let started = Instant::now();
            let result = git.refresh(&project, root.as_deref());
            let took = started.elapsed();
            let _ = events.blocking_send(Event::GitStatus { took, result });
        });
    }

    /// What a refresh found. A failure keeps the marks there are (they are only
    /// old), and only when asking again cannot help does the feature stop.
    pub(super) fn git_answer(&mut self, took: Duration, result: Result<Status, GitError>) {
        self.git.schedule.finished(Instant::now(), took);
        match result {
            Ok(status) => {
                self.git.root = Some(status.root.clone());
                self.git.told = false;
                if self.state.set_git_status(Some(status)) {
                    self.needs_redraw = true;
                }
            }
            Err(error) if error.is_final() => {
                self.git.off = true;
                self.git.root = None;
                if self.state.set_git_status(None) {
                    self.needs_redraw = true;
                }
                self.state.notify(format!("git is off: {error}"));
            }
            Err(error) => {
                if !std::mem::replace(&mut self.git.told, true) {
                    self.state.notify(format!(
                        "git could not be read, the marks may be old: {error}"
                    ));
                    self.needs_redraw = true;
                }
            }
        }
    }
}
