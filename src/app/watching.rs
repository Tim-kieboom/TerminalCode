//! Changes made outside the editor: which directories are watched, and what
//! to do when the watcher reports files.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::App;
use crate::components::workspace::DiskEvent;
use crate::config::ConfigFile;
use crate::files::paths::canonical;
use crate::files::watcher::{FsWatcher, WatchError};

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
        // The folder of the theme, keymap and layout files, to notice them being
        // saved by another program.
        let config = self
            .config_dir
            .as_deref()
            .filter(|dir| dir.is_dir())
            .map(canonical);
        let themes = self
            .config_dir
            .as_deref()
            .map(crate::config::themes_dir)
            .filter(|dir| dir.is_dir())
            .map(|dir| canonical(&dir));
        let wanted = || -> BTreeSet<PathBuf> {
            state
                .explorer()
                .open_directories()
                .iter()
                .map(|dir| canonical(dir))
                .chain(state.workspace().directories())
                .chain(config.clone())
                .chain(themes.clone())
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
        if self.config_file_among(paths) && self.config_hash() != self.config_applied {
            self.reload();
        }
        let events = self.state.workspace_mut().check_disk(paths);
        self.report_disk_events(events);
        self.refresh_explorer_for(paths);
        self.git_changed();
        self.push_file_text();
        self.needs_redraw = true;
    }

    /// Whether `paths` has the theme, the keymap or the layout file, which are
    /// read again when they are saved.
    fn config_file_among(&self, paths: &[PathBuf]) -> bool {
        let Some(dir) = self.config_dir.as_deref() else {
            return false;
        };
        let dir = canonical(dir);
        let installed = self.installed_theme_path().map(|path| canonical(&path));
        paths.iter().any(|path| {
            let path = canonical(path);
            installed.as_ref() == Some(&path)
                || ConfigFile::ALL
                    .iter()
                    .any(|file| path == file.path_in(&dir))
        })
    }

    /// Says what was done about the documents whose files changed.
    fn report_disk_events(&mut self, events: Vec<DiskEvent>) {
        for event in events {
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
                DiskEvent::ReadOnly(name, true) => self
                    .state
                    .notify(format!("{name} is read-only now; saving it is refused")),
                DiskEvent::ReadOnly(name, false) => {
                    self.state.notify(format!("{name} is writable again"));
                }
                DiskEvent::Unreadable(message) => self.state.notify_error(message),
            }
        }
    }

    /// Looks at the file of every open document, for a change no report told
    /// about (the watcher can miss one, and nothing watches while the editor is
    /// out of sight).
    pub(super) fn check_files_on_disk(&mut self) {
        let events = self.state.workspace_mut().check_all_disk();
        if !events.is_empty() {
            self.report_disk_events(events);
            self.needs_redraw = true;
        }
    }

    /// When another tab came to the front, looks at its file: the change may
    /// have happened while it was out of sight.
    pub(super) fn notice_tab_switch(&mut self) {
        let front = self.state.workspace().active_view();
        if front == self.front_view {
            return;
        }
        self.front_view = front;
        self.state.retarget_find();
        self.lsp_file_shown();
        if let Some(event) = self.state.workspace_mut().check_active_disk() {
            self.report_disk_events(vec![event]);
            self.needs_redraw = true;
        }
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
