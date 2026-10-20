//! Remembering which panes are hidden, between runs: see [`crate::persist::saved_state`].

use super::App;
use crate::persist::saved_state::{self, PluginRecord, SavedState};
use crate::ui::ComponentKind;

/// The panes that can be hidden and are remembered.
const PANES: [ComponentKind; 4] = [
    ComponentKind::Explorer,
    ComponentKind::Terminal,
    ComponentKind::StatusBar,
    ComponentKind::MenuBar,
];

/// What is remembered, and what was on screen the last time it was looked at.
#[derive(Debug)]
pub(super) struct Saved {
    state: SavedState,
    /// Whether each of [`PANES`] was shown when last looked at.
    seen: [bool; PANES.len()],
    /// Whether a failure to write the file has been told, so it is said once
    /// per run and not at every toggle.
    failure_told: bool,
}

impl Saved {
    pub(super) fn new(shown: [bool; PANES.len()]) -> Self {
        Self {
            state: SavedState::default(),
            seen: shown,
            failure_told: false,
        }
    }
}

impl App {
    /// Starts out knowing what was remembered; the panes are already as it says.
    pub(crate) fn with_saved_state(mut self, state: SavedState) -> Self {
        if let Some(percent) = state.side_percent() {
            self.state.set_side_percent(percent);
        }
        self.saved = Saved {
            state,
            seen: self.panes_shown(),
            failure_told: false,
        };
        self
    }

    /// Forgets what was remembered. The panes stay as they are on screen.
    pub(super) fn forget_saved_panes(&mut self) {
        self.saved.state = self.saved.state.reset();
    }

    /// What is remembered.
    pub(super) fn saved_state(&self) -> SavedState {
        self.saved.state.clone()
    }

    /// Remembers what the user decided about the plugin `id`.
    pub(super) fn remember_plugin(&mut self, id: &str, record: PluginRecord) {
        if self.saved.state.set_plugin(id, record) {
            self.write_saved_state();
        }
    }

    /// Remembers how wide the pane right of the editor is.
    pub(super) fn remember_side_percent(&mut self, percent: u16) {
        if self.saved.state.set_side_percent(percent) {
            self.write_saved_state();
        }
    }

    /// Remembers that the settings up to `version` were added to `settings.json`.
    pub(super) fn remember_settings_version(&mut self, version: &str) {
        if self.saved.state.set_settings_version(version) {
            self.write_saved_state();
        }
    }

    fn panes_shown(&self) -> [bool; PANES.len()] {
        PANES.map(|kind| self.state.is_visible(&kind))
    }

    /// Notes the panes the user hid or showed since this was last called, and
    /// writes the file when there are any. Called after every input and event, so
    /// that every way of hiding a pane is noticed.
    pub(super) fn remember_panes(&mut self) {
        let now = self.panes_shown();
        if now == self.saved.seen {
            return;
        }
        for ((kind, shown), before) in PANES.iter().zip(now).zip(self.saved.seen) {
            if shown != before {
                self.saved.state.set_shown(kind, shown);
            }
        }
        self.saved.seen = now;
        self.write_saved_state();
    }

    fn write_saved_state(&mut self) {
        let Some(dir) = self.config_dir.as_deref() else {
            return;
        };
        let Err(error) = saved_state::save(dir, &self.saved.state) else {
            return;
        };
        if !self.saved.failure_told {
            self.saved.failure_told = true;
            let path = saved_state::path_in(dir);
            self.state.notify_error(format!(
                "{}: {error}; what is hidden will not be remembered",
                path.display()
            ));
        }
    }
}
