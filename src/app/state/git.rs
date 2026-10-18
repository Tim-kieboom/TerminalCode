//! What git said, kept for drawing: the branch and the changed files.

use std::path::Path;

use super::AppState;
use crate::git::{Mark, Marks, Status};

impl AppState {
    /// The repository's state as of the last refresh; `None` before the first
    /// one, and when there is no repository.
    pub(crate) fn git_status(&self) -> Option<&Status> {
        self.git.as_ref()
    }

    /// Replaces the state; returns whether it differs from the one there was.
    pub(crate) fn set_git_status(&mut self, status: Option<Status>) -> bool {
        let changed = self.git != status;
        if changed {
            self.git_marks = status.as_ref().map(Marks::from_status).unwrap_or_default();
            self.git = status;
        }
        changed
    }

    /// What shows beside the file or folder at `path`, when git lists it or
    /// something in it.
    pub(crate) fn git_mark(&self, path: &Path) -> Option<Mark> {
        self.git_marks.of(path)
    }
}
