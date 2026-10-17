//! Open Recent: noting the files and folders opened, and the menu that lists them.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::App;
use crate::components::popups::menu::Entry;
use crate::event::action::Action;
use crate::paths::canonical;
use crate::recent::{self, Recent};

/// The list, what has been noted already in this run, and whether a failure to
/// write it has been told.
#[derive(Debug, Default)]
pub(super) struct Recents {
    list: Recent,
    /// The files of the documents seen so far: one is noted when it first appears.
    seen: HashSet<PathBuf>,
    /// The project folder is noted once, at the first look.
    started: bool,
    failure_told: bool,
}

impl App {
    /// Starts out with the list that was saved.
    pub(crate) fn with_recent(mut self, list: Recent) -> Self {
        self.recent.list = list;
        self
    }

    /// Notes the project folder and every file that came into a tab since the last
    /// call, and writes the list when it changed. Called after every input and
    /// event, so that every way of opening a file is noticed.
    pub(super) fn remember_recent(&mut self) {
        let mut changed = false;
        if !self.recent.started {
            self.recent.started = true;
            if let Some(root) = self.state.explorer().root_path() {
                changed |= self.recent.list.push_folder(&canonical(root));
            }
        }
        for path in self.state.workspace().open_paths() {
            let path = canonical(&path);
            if self.recent.seen.insert(path.clone()) {
                changed |= self.recent.list.push_file(&path);
            }
        }
        if changed {
            self.write_recent();
        }
    }

    fn write_recent(&mut self) {
        let Some(dir) = self.config_dir.as_deref() else {
            return;
        };
        let Err(error) = recent::save(dir, &self.recent.list) else {
            return;
        };
        if !self.recent.failure_told {
            self.recent.failure_told = true;
            self.state.notify_error(format!(
                "{}: {error}; the recent files will not be remembered",
                recent::path_in(dir).display()
            ));
        }
    }

    /// The entries of the Open Recent menu: the folders, the files, and Clear Recent.
    /// What is gone from the disk is dropped first.
    pub(super) fn recent_entries(&mut self) -> Vec<Entry> {
        if self.recent.list.prune() {
            self.write_recent();
        }
        let item = |path: &Path, action: Action| Entry::Item {
            label: shown(path).into(),
            action,
            keys: None,
        };
        let list = &self.recent.list;
        let mut entries: Vec<Entry> = list
            .folders()
            .iter()
            .map(|path| item(path, Action::OpenRecentFolder(text(path).into())))
            .collect();
        if !list.folders().is_empty() && !list.files().is_empty() {
            entries.push(Entry::Separator);
        }
        entries.extend(
            list.files()
                .iter()
                .map(|path| item(path, Action::OpenRecentFile(text(path).into()))),
        );
        if !entries.is_empty() {
            entries.push(Entry::Separator);
        }
        entries.push(Entry::Item {
            label: if list.is_empty() {
                "Nothing opened yet".into()
            } else {
                "Clear Recent".into()
            },
            action: Action::ClearRecent,
            keys: None,
        });
        entries
    }

    pub(super) fn open_recent_file(&mut self, path: &str) {
        if let Err(error) = self.open_file_at(Path::new(path)) {
            self.state.notify_error(error.to_string());
        }
    }

    pub(super) fn open_recent_folder(&mut self, path: &str) {
        if let Err(error) = self.restart_in_folder(Path::new(path)) {
            self.state.notify_error(error.to_string());
        }
    }

    pub(super) fn clear_recent(&mut self) {
        if self.recent.list.clear() {
            self.write_recent();
        }
    }

    #[cfg(test)]
    pub(crate) fn recent_list(&self) -> &Recent {
        &self.recent.list
    }
}

/// The path as the action carries it.
fn text(path: &Path) -> String {
    path.display().to_string()
}

/// The path as the menu shows it: the home folder written as `~`.
fn shown(path: &Path) -> String {
    match dirs::home_dir().and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf)) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()).replace('\\', "/"),
        None => path.display().to_string(),
    }
}
