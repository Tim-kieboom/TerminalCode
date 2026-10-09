//! Changes made outside the editor: which directories are watched, and what
//! to do when the watcher reports files.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::App;
use crate::components::workspace::DiskEvent;
use crate::paths::canonical;
use crate::watcher::{FsWatcher, WatchError};

/// The watcher, if there is one, and what it was last set up for.
#[derive(Debug, Default)]
pub(super) struct Watching {
    watcher: Option<FsWatcher>,
    /// The explorer and document versions the watches were last set for.
    versions: Option<(u64, u64)>,
    /// Whether a failure was already handed out, so it is shown once.
    failure_given: bool,
}

impl Watching {
    fn attach(&mut self, watcher: FsWatcher) {
        self.watcher = Some(watcher);
    }

    /// Makes the watched directories `wanted()`, if `versions` is not what they
    /// were last set for. Returns the error the first time watching fails, and
    /// nothing for later ones.
    fn sync(
        &mut self,
        versions: (u64, u64),
        wanted: impl FnOnce() -> BTreeSet<PathBuf>,
    ) -> Option<WatchError> {
        let watcher = self.watcher.as_mut()?;
        if self.versions == Some(versions) {
            return None;
        }
        self.versions = Some(versions);

        let error = watcher.watch_only(&wanted()).err()?;
        let already_shown = std::mem::replace(&mut self.failure_given, true);
        (!already_shown).then_some(error)
    }

    /// Drops the watches of the directories `paths` names itself, which the
    /// operating system lost when they were deleted or replaced; the next
    /// [`Watching::sync`] sets them up again.
    fn forget(&mut self, paths: &[PathBuf]) {
        let Some(watcher) = self.watcher.as_mut() else {
            return;
        };
        if watcher.forget(paths) {
            self.versions = None;
        }
    }

    #[cfg(test)]
    fn watched(&self) -> Vec<PathBuf> {
        self.watcher
            .as_ref()
            .map(|watcher| watcher.watched().iter().cloned().collect())
            .unwrap_or_default()
    }
}

impl App {
    /// Watches the directories of open files and of the explorer for changes
    /// made outside the editor.
    pub(crate) fn with_watcher(mut self, watcher: FsWatcher) -> Self {
        self.watching.attach(watcher);
        self.sync_watches();
        self
    }

    #[cfg(test)]
    pub(crate) fn watched_directories(&self) -> Vec<PathBuf> {
        self.watching.watched()
    }

    /// Makes the watched directories match what is open. Cheap when nothing
    /// was opened or closed since the last call.
    pub(crate) fn sync_watches(&mut self) {
        let versions = (
            self.state.explorer().version(),
            self.state.workspace().documents_version(),
        );
        let state = &self.state;
        let wanted = || -> BTreeSet<PathBuf> {
            state
                .explorer()
                .open_directories()
                .iter()
                .map(|dir| canonical(dir))
                .chain(state.workspace().directories())
                .collect()
        };
        if let Some(error) = self.watching.sync(versions, wanted) {
            self.state.notify_error(format!(
                "changes made outside the editor may be missed: {error}"
            ));
            self.needs_redraw = true;
        }
    }

    /// Files changed outside the editor: brings open documents in step (see
    /// [`crate::components::workspace::Workspace::check_disk`]) and refreshes
    /// the explorer.
    pub(super) fn files_changed(&mut self, paths: &[PathBuf]) {
        self.rewatch_replaced(paths);
        for event in self.state.workspace_mut().check_disk(paths) {
            match event {
                DiskEvent::Reloaded(name) => {
                    self.state
                        .notify(format!("{name} changed on disk; reloaded it"));
                }
                DiskEvent::Conflict(name) => self.state.notify_error(format!(
                    "{name} changed on disk; your changes are kept (saving asks before overwriting)"
                )),
                DiskEvent::Deleted(name) => {
                    self.state.notify(format!("{name} was deleted on disk"));
                }
                DiskEvent::Unreadable(message) => self.state.notify_error(message),
            }
        }
        self.refresh_explorer_for(paths);
        self.needs_redraw = true;
    }

    /// Re-reads the explorer's directories that `paths` are in, and only those.
    fn refresh_explorer_for(&mut self, paths: &[PathBuf]) {
        if let Err(error) = self.state.explorer_mut().refresh_paths(paths) {
            self.state.notify_error(error.to_string());
        }
    }

    /// Starts watching again the directories the report says were replaced.
    fn rewatch_replaced(&mut self, paths: &[PathBuf]) {
        self.watching.forget(paths);
    }
}

#[cfg(test)]
mod tests;
