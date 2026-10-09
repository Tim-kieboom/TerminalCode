//! The command palette.

use crossterm::event::KeyEvent;

use crate::app::App;
use crate::app::keys::PALETTE_CONTEXTS;
use crate::components::palette::{self, Entry, Palette};
use crate::event::action::Action;

impl App {
    pub(in crate::app) fn open_palette(&mut self) {
        let entries = Action::palette_actions()
            .into_iter()
            .filter_map(|action| {
                let title = action.title()?;
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
        self.state.open_palette(Palette::new(entries));
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
            self.run_action(action);
        }
    }
}
