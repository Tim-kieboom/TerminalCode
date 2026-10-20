//! What the editor remembers between runs, in `state.json` in the config folder:
//! which panes the user hid or showed. It is a file of its own, not part of
//! `settings.json`, which the user edits by hand and the editor only ever adds new settings to
//! (see `settings::update`).
//!
//! Only what the user changed is written. A pane with nothing remembered starts
//! the way the layout file says (`hidden: true`), and otherwise the way it always
//! did, so editing the layout still moves a pane the user never touched.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::ConfigWarning;
use crate::ui::ComponentKind;

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

/// Where the user put a plugin's view: in the bar that holds the Explorer, in the
/// bar that holds the Terminal, or in a popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Place {
    Explorer,
    Bottom,
    Popup,
    /// A place a later version has, which this one cannot put anything in.
    #[serde(other)]
    Unknown,
}

impl Place {
    /// The name in an action: `explorer`, `bottom` or `popup`.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Explorer => "explorer",
            Self::Bottom => "bottom",
            Self::Popup => "popup",
            Self::Unknown => "unknown",
        }
    }

    /// The place an action names.
    pub(crate) fn from_key(key: &str) -> Option<Self> {
        [Self::Explorer, Self::Bottom, Self::Popup]
            .into_iter()
            .find(|place| place.key() == key)
    }

    /// The words the manager uses for it.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Explorer => "Explorer's bar",
            Self::Bottom => "bottom bar",
            Self::Popup => "popup",
            Self::Unknown => "somewhere this version does not have",
        }
    }
}

/// What the user decided about a plugin.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PluginRecord {
    /// Whether the plugin runs.
    pub enabled: bool,
    /// The names of the capabilities the user was shown when they decided; a
    /// plugin that comes to need more is asked about again.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// The file types (`file_types` of its manifest) the user was shown when it asked
    /// for `files.text`; a plugin that comes to want more is asked about again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_types: Vec<String>,
    /// Whether the user was shown that the plugin runs on a thread of its own and may
    /// use a lot of processor time (`thread = "own"`); a plugin that comes to ask for it
    /// is asked about again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub own_thread: bool,
    /// Where its view goes; `None` until the user chose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<Place>,
}

/// The remembered state. Names in the file that this version does not know are
/// left alone when reading, so a file from a newer version still loads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SavedState {
    #[serde(default)]
    shown: Shown,
    /// The newest version whose settings were added to the user's `settings.json`
    /// (see `settings::update`). It is not forgotten by a reset: a setting the user
    /// deleted would come back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    settings_version: Option<String>,
    /// What the user decided about each plugin, by id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    plugins: BTreeMap<String, PluginRecord>,
}

impl SavedState {
    /// Whether the pane was last shown (`true`) or hidden, if it was ever changed.
    pub(crate) fn shown(&self, kind: &ComponentKind) -> Option<bool> {
        let mut shown = self.shown;
        shown.slot(kind).and_then(|slot| *slot)
    }

    /// The version whose settings were last added to `settings.json`, if any.
    pub(crate) fn settings_version(&self) -> Option<&str> {
        self.settings_version.as_deref()
    }

    /// Remembers that the settings up to `version` were added. Returns whether that is news.
    pub(crate) fn set_settings_version(&mut self, version: &str) -> bool {
        let changed = self.settings_version.as_deref() != Some(version);
        self.settings_version = Some(version.to_owned());
        changed
    }

    /// What a reset leaves: no pane and no plugin remembered (plugins are asked
    /// about again), but the settings version kept.
    pub(crate) fn reset(&self) -> Self {
        Self {
            shown: Shown::default(),
            settings_version: self.settings_version.clone(),
            plugins: BTreeMap::new(),
        }
    }

    /// What the user decided about every plugin, by id.
    pub(crate) fn plugins(&self) -> impl Iterator<Item = (&str, &PluginRecord)> {
        self.plugins
            .iter()
            .map(|(id, record)| (id.as_str(), record))
    }

    /// What the user decided about the plugin `id`, if they ever did.
    pub(crate) fn plugin(&self, id: &str) -> Option<&PluginRecord> {
        self.plugins.get(id)
    }

    /// Remembers what the user decided about the plugin `id`. Returns whether that
    /// is news.
    pub(crate) fn set_plugin(&mut self, id: &str, record: PluginRecord) -> bool {
        self.plugins.insert(id.to_owned(), record.clone()) != Some(record)
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

/// The text of a file that remembers nothing but what a reset keeps.
pub(crate) fn empty_text(state: &SavedState) -> String {
    let text = serde_json::to_string_pretty(&state.reset()).unwrap_or_default();
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
