//! What the editor remembers between runs, in `state.json` in the config folder:
//! which panes the user hid or showed. It is a file of its own, not part of
//! `settings.json`, which the user edits by hand and the editor never writes.
//!
//! Only what the user changed is written. A pane with nothing remembered starts
//! the way the layout file says (`hidden: true`), and otherwise the way it always
//! did, so editing the layout still moves a pane the user never touched.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::components::ComponentKind;
use crate::config::ConfigWarning;

pub(crate) const STATE_FILE: &str = "state.json";

/// Whether each pane that can be hidden was last shown (`true`) or hidden.
/// `None` is a pane the user never changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Shown {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    explorer: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    terminal: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    status_bar: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    menu_bar: Option<bool>,
}

impl Shown {
    fn slot(&mut self, kind: &ComponentKind) -> Option<&mut Option<bool>> {
        match kind {
            ComponentKind::Explorer => Some(&mut self.explorer),
            ComponentKind::Terminal => Some(&mut self.terminal),
            ComponentKind::StatusBar => Some(&mut self.status_bar),
            ComponentKind::MenuBar => Some(&mut self.menu_bar),
            ComponentKind::Editor | ComponentKind::Errors | ComponentKind::Plugin(_) => None,
        }
    }
}

/// The remembered state. Names in the file that this version does not know are
/// left alone when reading, so a file from a newer version still loads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SavedState {
    #[serde(default)]
    shown: Shown,
}

impl SavedState {
    /// Whether the pane was last shown (`true`) or hidden, if it was ever changed.
    pub(crate) fn shown(&self, kind: &ComponentKind) -> Option<bool> {
        let mut shown = self.shown;
        shown.slot(kind).and_then(|slot| *slot)
    }

    /// Remembers that the pane is `shown` now. Returns whether that is news.
    pub(crate) fn set_shown(&mut self, kind: &ComponentKind, shown: bool) -> bool {
        let Some(slot) = self.shown.slot(kind) else {
            return false;
        };
        let changed = *slot != Some(shown);
        *slot = Some(shown);
        changed
    }
}

/// Where the file is inside the config folder `dir`.
pub(crate) fn path_in(dir: &Path) -> PathBuf {
    dir.join(STATE_FILE)
}

/// The text of a file that remembers nothing.
pub(crate) fn empty_text() -> String {
    let text = serde_json::to_string_pretty(&SavedState::default()).unwrap_or_default();
    format!("{text}\n")
}

/// The remembered state in `dir`. A missing file is normal; a file that cannot
/// be read or is not valid gives the empty state and a warning.
pub(crate) fn load(dir: Option<&Path>) -> (SavedState, Option<ConfigWarning>) {
    let Some(dir) = dir else {
        return (SavedState::default(), None);
    };
    let path = path_in(dir);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return (SavedState::default(), None);
        }
        Err(error) => {
            return (
                SavedState::default(),
                Some(ConfigWarning::unusable(&path, error, "pane visibility")),
            );
        }
    };
    match serde_json::from_str(&text) {
        Ok(state) => (state, None),
        Err(error) => (
            SavedState::default(),
            Some(ConfigWarning::unusable(&path, error, "pane visibility")),
        ),
    }
}

/// Writes the state to `dir`. The text goes to a file beside it first and is
/// then renamed over it, so a crash leaves the old file or the new one, never half
/// of one.
pub(crate) fn save(dir: &Path, state: &SavedState) -> io::Result<()> {
    let text = serde_json::to_string_pretty(state).map_err(io::Error::other)?;
    write_atomically(&path_in(dir), &format!("{text}\n"))
}

/// Writes `text` to `path` through a file beside it and a rename, making the
/// folder when it is not there yet.
pub(crate) fn write_atomically(path: &Path, text: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}

#[cfg(test)]
mod tests;
