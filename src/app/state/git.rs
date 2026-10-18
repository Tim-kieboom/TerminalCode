//! What git said, kept for drawing: the branch and the changed files.

use super::AppState;
use crate::git::Status;

impl AppState {
    /// The repository's state as of the last refresh; `None` before the first
    /// one, and when there is no repository.
    pub(crate) fn git_status(&self) -> Option<&Status> {
        self.git.as_ref()
    }

    /// Replaces the state; returns whether it differs from the one there was.
    pub(crate) fn set_git_status(&mut self, status: Option<Status>) -> bool {
        let changed = self.git != status;
        self.git = status;
        changed
    }
}
