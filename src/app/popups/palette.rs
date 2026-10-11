//! The command palette.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::app::App;
use crate::app::keys::PALETTE_CONTEXTS;
use crate::components::PluginViewId;
use crate::components::palette::{self, Entry, Palette};
use crate::event::action::Action;

/// Most characters pasted into a popup's text.
const MAX_PASTE: usize = 200;

impl App {
    pub(in crate::app) fn open_palette(&mut self) {
        let mut actions = Action::palette_actions();
        // One entry for each installed theme.
        if let Some(dir) = self.config_dir.as_deref() {
            actions.extend(
                crate::config::installed_themes(dir)
                    .into_iter()
                    .map(|name| Action::UseInstalledTheme(name.into())),
            );
        }
        // One entry for each plugin view that exists, to show or hide it.
        actions.extend(
            self.state
                .plugin_view_ids()
                .into_iter()
                .map(|id| Action::TogglePluginView(id.as_str().into())),
        );
        let entries = actions
            .into_iter()
            .filter_map(|action| {
                let title = self.palette_title(&action)?;
                let keys = self
                    .keyboard
                    .keys_for(&action, self.contexts())
                    .or_else(|| self.keyboard.keys_for(&action, &PALETTE_CONTEXTS));
                Some(Entry {
                    title,
                    action,
                    keys,
                })
            })
            .collect();
        let palette = Palette::with_recent(entries, self.state.palette_recent());
        self.state.open_palette(palette);
    }

    /// Opens the palette on the themes: the built-in ones and the installed.
    pub(in crate::app) fn choose_theme(&mut self) {
        self.open_palette();
        if let Some(mut palette) = self.state.take_palette() {
            "Theme: ".chars().for_each(|c| palette.insert(c));
            self.state.open_palette(palette);
        }
    }

    /// What the palette calls `action`; the view toggles are named by the view.
    fn palette_title(&self, action: &Action) -> Option<String> {
        match action {
            Action::UseInstalledTheme(name) => Some(format!("Theme: {name}")),
            Action::TogglePluginView(id) => {
                let known = self.state.plugin_view(&PluginViewId::new(id.as_ref()));
                known.map(|_| format!("View: Toggle {id}"))
            }
            other => other.title().map(str::to_owned),
        }
    }

    pub(in crate::app) fn handle_palette_key(&mut self, key: KeyEvent) {
        let Some(command) = palette::Command::from_key(key) else {
            return;
        };
        let Some(mut palette) = self.state.take_palette() else {
            return;
        };
        let mut keep_open = true;
        let mut chosen = None;
        match command {
            palette::Command::Insert(c) => palette.insert(c),
            palette::Command::Backspace => palette.backspace(),
            palette::Command::ClearQuery => palette.clear_query(),
            palette::Command::Previous => palette.select_previous(),
            palette::Command::Next => palette.select_next(),
            palette::Command::Cancel => keep_open = false,
            palette::Command::Run => {
                chosen = palette.selected_entry().map(|entry| entry.action.clone());
                keep_open = chosen.is_none();
            }
        }
        if keep_open {
            self.state.open_palette(palette);
        }
        self.needs_redraw = true;
        if let Some(action) = chosen {
            self.run_from_palette(action);
        }
    }

    /// A mouse event while the palette is open. Returns whether the palette
    /// used it up (it always does: nothing behind it takes clicks).
    pub(in crate::app) fn handle_palette_mouse(&mut self, event: MouseEvent) -> bool {
        let Some(mut palette) = self.state.take_palette() else {
            return false;
        };
        let screen = self.state.screen();
        let mut chosen = None;
        let mut keep_open = true;
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                match palette.row_at(screen, event.column, event.row) {
                    Some(index) => {
                        palette.select(index);
                        chosen = palette.selected_entry().map(|entry| entry.action.clone());
                        keep_open = false;
                    }
                    // A click outside the box closes it; one on the box does nothing.
                    None => keep_open = palette.contains(screen, event.column, event.row),
                }
            }
            MouseEventKind::ScrollUp => palette.select_previous(),
            MouseEventKind::ScrollDown => palette.select_next(),
            _ => {}
        }
        if keep_open {
            self.state.open_palette(palette);
        }
        if let Some(action) = chosen {
            self.run_from_palette(action);
        }
        self.needs_redraw = true;
        true
    }

    /// Runs what the palette chose, and remembers it for next time.
    fn run_from_palette(&mut self, action: Action) {
        self.state.remember_palette_action(action.clone());
        self.run_action(action);
    }

    /// Text pasted while a popup with a text line is open goes into that line,
    /// as typed: the first line of it, and not more than a few hundred
    /// characters. Returns whether a popup took it.
    pub(in crate::app) fn paste_into_popup(&mut self, text: &str) -> bool {
        use crate::app::state::Popup;
        if !matches!(
            self.state.popup(),
            Popup::Palette(_)
                | Popup::Finder(_)
                | Popup::Search(_)
                | Popup::NewEntry(_)
                | Popup::Find(_)
        ) {
            return false;
        }
        let first_line = text.lines().next().unwrap_or_default();
        for c in first_line
            .chars()
            .filter(|c| !c.is_control())
            .take(MAX_PASTE)
        {
            // As if typed: each popup does what it does for a typed character.
            self.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        self.needs_redraw = true;
        true
    }
}
