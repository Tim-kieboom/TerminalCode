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
    /// Counts the statuses that differed from the one before.
    generation: u64,
    /// The status and the open documents the index texts were last asked for.
    bases_for: Option<(u64, u64)>,
}

impl App {
    /// Says that what git shows may have changed: a refresh follows soon, and
    /// not before the one that is running ended.
    pub(crate) fn git_changed(&mut self) {
        self.git_wanted(false);
    }

    /// Says that what git shows changed because of something the editor just
    /// did (a plugin staged a file): the refresh does not wait for a burst of
    /// changes to end.
    pub(super) fn git_changed_now(&mut self) {
        self.git_wanted(true);
    }

    fn git_wanted(&mut self, now: bool) {
        if self.git.off || !self.background.is_connected() {
            return;
        }
        match now {
            true => self.git.schedule.request_now(Instant::now()),
            false => self.git.schedule.request(Instant::now()),
        }
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
                    self.git.generation += 1;
                    self.publish_git_to_plugins();
                    self.needs_redraw = true;
                }
                self.git_bases_step();
            }
            Err(error) if error.is_final() => {
                self.git.off = true;
                self.git.root = None;
                self.state.workspace_mut().clear_git_lines();
                if self.state.set_git_status(None) {
                    self.needs_redraw = true;
                }
                self.publish_git_to_plugins();
                self.state.notify(format!("git is off: {error}"));
            }
            Err(error) => self.git_problem(&error),
        }
    }

    /// Tells of a problem that is not final, once until something works again.
    /// The generation of the state of the repository, for what plugins are told.
    pub(crate) fn git_generation(&self) -> u64 {
        self.git.generation
    }

    /// The directory of the repository and the way to run `git`, for work in the
    /// background.
    pub(super) fn git_tool(&self) -> (Git, Option<std::path::PathBuf>) {
        (self.git.git.clone(), self.git.root.clone())
    }

    /// Runs this `git` instead of the user's, for tests.
    #[cfg(test)]
    pub(crate) fn with_git_tool(mut self, git: Git) -> Self {
        self.git.git = git;
        self
    }

    fn git_problem(&mut self, error: &GitError) {
        if !std::mem::replace(&mut self.git.told, true) {
            self.state.notify(format!(
                "git could not be read, the marks may be old: {error}"
            ));
            self.needs_redraw = true;
        }
    }

    /// Asks, in the background, for the index text of each open file whose
    /// text git may have changed since it was last asked for: the first time,
    /// and when git says the file differs in another way (a file that was staged
    /// has another index text). The lines that differ show in the gutter.
    pub(crate) fn git_bases_step(&mut self) {
        let versions = (
            self.git.generation,
            self.state.workspace().documents_version(),
        );
        if self.git.bases_for == Some(versions) {
            return;
        }
        let (Some(events), Some(root)) = (
            self.background.sender(),
            self.state.git_status().map(|status| status.root.clone()),
        ) else {
            return;
        };
        self.git.bases_for = Some(versions);

        let marks = self.state.git_marks();
        let documents: Vec<_> = self
            .state
            .workspace()
            .document_paths()
            .into_iter()
            .map(|(id, path)| (id, marks.relative(&path), marks.change_of(&path)))
            .collect();
        for (id, relative, change) in documents {
            let Some(lines) = self.state.workspace_mut().git_lines_mut(id) else {
                continue;
            };
            let Some(relative) = relative else {
                lines.clear();
                continue;
            };
            if !lines.wants(&relative, change) {
                continue;
            }
            let request = lines.ask(relative.clone(), change);
            let (git, root, events) = (self.git.git.clone(), root.clone(), events.clone());
            std::thread::spawn(move || {
                let result = git.index_lines(&root, &relative);
                let _ = events.blocking_send(Event::GitBase {
                    document: id,
                    request,
                    result,
                });
            });
        }
    }

    /// The index text a request asked for, or why there is none.
    pub(super) fn git_base_answer(
        &mut self,
        document: crate::ids::DocumentId,
        request: u64,
        result: Result<Option<Vec<u64>>, GitError>,
    ) {
        let Some(lines) = self.state.workspace_mut().git_lines_mut(document) else {
            return;
        };
        match result {
            Ok(base) => {
                if lines.accept(request, base) {
                    self.needs_redraw = true;
                }
            }
            Err(error) => {
                lines.failed(request);
                self.git_problem(&error);
            }
        }
    }
}
