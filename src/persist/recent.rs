//! The files and folders opened lately, for the main menu's Open Recent. They are
//! kept in `recent.json` in the config folder, a file of its own: not in
//! `settings.json`, which the user edits by hand and the editor only ever adds new settings to
//! (see `settings::update`).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::ConfigWarning;

pub(crate) const RECENT_FILE: &str = "recent.json";

/// How many files and how many folders are kept.
pub(crate) const MOST: usize = 10;

/// The latest first, each path once.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Recent {
    #[serde(default)]
    files: Vec<PathBuf>,
    #[serde(default)]
    folders: Vec<PathBuf>,
}

impl Recent {
    pub(crate) fn files(&self) -> &[PathBuf] {
        &self.files
    }

    pub(crate) fn folders(&self) -> &[PathBuf] {
        &self.folders
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.files.is_empty() && self.folders.is_empty()
    }

    /// Notes a file that was opened. Returns whether the list changed.
    pub(crate) fn push_file(&mut self, path: &Path) -> bool {
        push(&mut self.files, path)
    }

    /// Notes a folder that was opened as the project. Returns whether the list changed.
    pub(crate) fn push_folder(&mut self, path: &Path) -> bool {
        push(&mut self.folders, path)
    }

    /// Drops what is not there any more. Returns whether anything was dropped.
    pub(crate) fn prune(&mut self) -> bool {
        let before = self.files.len() + self.folders.len();
        self.files.retain(|path| path.is_file());
        self.folders.retain(|path| path.is_dir());
        self.files.len() + self.folders.len() != before
    }

    /// Forgets everything. Returns whether there was anything.
    pub(crate) fn clear(&mut self) -> bool {
        let had = !self.is_empty();
        *self = Self::default();
        had
    }
}

/// Puts `path` first, once, and keeps at most [`MOST`]. A path that is first
/// already changes nothing.
fn push(list: &mut Vec<PathBuf>, path: &Path) -> bool {
    if list.first().is_some_and(|first| first == path) {
        return false;
    }
    list.retain(|known| known != path);
    list.insert(0, path.to_owned());
    list.truncate(MOST);
    true
}

/// Where the file is inside the config folder `dir`.
pub(crate) fn path_in(dir: &Path) -> PathBuf {
    dir.join(RECENT_FILE)
}

/// The list in `dir`. A missing file is normal; one that cannot be read or is not
/// valid gives an empty list and a warning (the next change writes a new file).
pub(crate) fn load(dir: Option<&Path>) -> (Recent, Option<ConfigWarning>) {
    let Some(dir) = dir else {
        return (Recent::default(), None);
    };
    let path = path_in(dir);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return (Recent::default(), None);
        }
        Err(error) => {
            return (
                Recent::default(),
                Some(ConfigWarning::unusable(&path, error, "recent files")),
            );
        }
    };
    match serde_json::from_str::<Recent>(&text) {
        Ok(mut recent) => {
            recent.files.truncate(MOST);
            recent.folders.truncate(MOST);
            (recent, None)
        }
        Err(error) => (
            Recent::default(),
            Some(ConfigWarning::unusable(&path, error, "recent files")),
        ),
    }
}

/// Writes the list to `dir`, through a temporary file and a rename.
pub(crate) fn save(dir: &Path, recent: &Recent) -> io::Result<()> {
    let text = serde_json::to_string_pretty(recent).map_err(io::Error::other)?;
    crate::persist::saved_state::write_atomically(&path_in(dir), &format!("{text}\n"))
}
