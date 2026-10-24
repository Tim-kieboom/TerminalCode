//! Choosing the run configuration: the picker, what is remembered about the choice for each
//! project, and the name shown in the status bar. See `run.rs` for what Run does with it.

use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};

use super::App;
use super::plugin_trust::trust_key;
use crate::components::popups::action_list::{Command, Entry};
use crate::launch::{ConfigId, RunConfig, What};

impl App {
    /// What identifies the project folder in what is remembered.
    fn run_project_key(&self) -> String {
        trust_key(&self.project_root())
    }

    /// The configuration the Run key starts: the one chosen in this run of the editor, or the
    /// one remembered for the project.
    pub(super) fn active_run_id(&self) -> Option<ConfigId> {
        if let Some(id) = &self.runs.active {
            return Some(id.clone());
        }
        let remembered = self.saved_active_run(&self.run_project_key())?;
        Some(ConfigId::from(remembered.as_str()))
    }

    /// Makes `id` the configuration the Run key starts, and remembers it for the project.
    pub(super) fn set_active_run(&mut self, id: &ConfigId) {
        self.runs.active = Some(id.clone());
        let project = self.run_project_key();
        self.remember_active_run(&project, id.as_str());
        self.refresh_run_label();
    }

    /// What the status bar says about Run: the name of the configuration the Run key starts
    /// (nothing until the list is worked out, or when it has nothing to run).
    pub(super) fn refresh_run_label(&mut self) {
        let label = self
            .effective_config()
            .map(|config| config.name().to_owned());
        self.state.set_run_label(label);
    }

    /// "Run: Choose Configuration...": opens the picker (when the list is worked out).
    pub(super) fn choose_run(&mut self) {
        if self.runs.list.is_none() {
            self.runs.picker_wanted = true;
            self.refresh_run_list();
            return;
        }
        self.open_run_picker();
    }

    /// Opens the picker with a row for each configuration, the active one marked and selected.
    pub(super) fn open_run_picker(&mut self) {
        let Some(list) = self.runs.list.as_ref() else {
            return;
        };
        if list.is_empty() {
            let why = match list.note() {
                Some(note) => format!("nothing to run: {note}"),
                None => "nothing to run: the project has no binary or example, and \
                         .trcode/launch.toml has no command"
                    .to_owned(),
            };
            self.state.notify(why);
            return;
        }
        let active = self.effective_config().map(|config| config.id().clone());
        let mut ids = Vec::new();
        let mut entries = Vec::new();
        let mut selected = 0;
        for (at, config) in list.iter().enumerate() {
            if active.as_ref() == Some(config.id()) {
                selected = at;
            }
            ids.push(config.id().clone());
            entries.push(entry_of(config, active.as_ref() == Some(config.id())));
        }
        self.runs.picker = ids;
        self.state
            .open_run_picker(crate::components::popups::action_list::ActionList::titled(
                entries,
                "Run configurations",
                selected,
            ));
        self.needs_redraw = true;
    }

    /// A key while the picker is open.
    pub(super) fn handle_run_picker_key(&mut self, key: KeyEvent) {
        let Some(command) = Command::from_key(key) else {
            return;
        };
        let Some(list) = self.state.run_picker_mut() else {
            return;
        };
        match command {
            Command::Previous => list.step(-1),
            Command::Next => list.step(1),
            Command::PageUp => list.page(-1),
            Command::PageDown => list.page(1),
            Command::Go => self.pick_run(),
            Command::Pick(number) => {
                if list.select_number(number) {
                    self.pick_run();
                }
            }
            Command::Cancel => {
                self.state.take_run_picker();
                self.runs.picker.clear();
            }
        }
        self.needs_redraw = true;
    }

    /// The mouse while the picker is open: a click on a row picks it, one outside closes the
    /// box, the wheel moves the selection. Whether the picker was open.
    pub(super) fn handle_run_picker_mouse(&mut self, event: MouseEvent) -> bool {
        let Some(list) = self.state.run_picker_mut() else {
            return false;
        };
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if list.select_at(event.column, event.row) {
                    self.pick_run();
                } else if !list.contains(event.column, event.row) {
                    self.state.take_run_picker();
                    self.runs.picker.clear();
                }
            }
            MouseEventKind::ScrollDown => list.step(1),
            MouseEventKind::ScrollUp => list.step(-1),
            _ => {}
        }
        true
    }

    /// The selected row is chosen: it becomes the active configuration and runs. One that
    /// cannot be run says why and leaves the picker open.
    fn pick_run(&mut self) {
        let Some(at) = self.state.run_picker_mut().map(|list| list.selected()) else {
            return;
        };
        let Some(id) = self.runs.picker.get(at).cloned() else {
            return;
        };
        let unavailable = self
            .runs
            .list
            .as_ref()
            .and_then(|list| list.get(&id))
            .and_then(|config| config.unavailable().map(|why| (config.name(), why)));
        if let Some((name, why)) = unavailable {
            let message = format!("{name} cannot be run: {why}");
            self.state.notify(message);
            return;
        }
        self.state.take_run_picker();
        self.runs.picker.clear();
        self.set_active_run(&id);
        self.run_active();
    }
}

/// The row of a configuration: its name (a mark before the active one), where it is from, and
/// why it cannot be run when it cannot.
fn entry_of(config: &RunConfig, active: bool) -> Entry {
    let mark = if active { "● " } else { "  " };
    let kind = match config.what() {
        What::Cargo(target) => target.id().package().to_owned(),
        What::Command(_) => "command".to_owned(),
        What::Missing(id) => id.package().to_owned(),
    };
    Entry {
        title: format!("{mark}{}", config.name()),
        kind,
        disabled: config.unavailable().map(str::to_owned),
    }
}
