//! The file finder.

use crossterm::event::KeyEvent;

use crate::app::App;
use crate::components::finder::{self, Finder};
use crate::components::palette;
use crate::state::Focus;

impl App {
    /// Opens the file finder on the project and starts the walk that feeds it.
    pub(in crate::app) fn open_finder(&mut self) {
        let Some(events) = self.background.sender() else {
            self.state
                .notify_error("the file finder needs the event loop, which is not running");
            return;
        };
        let root = self.project_root();
        let scan = self.background.next_scan();
        let handle = finder::start_scan(root.clone(), scan, events);
        self.state
            .open_finder(Finder::new(root, scan, Some(handle)));
    }

    pub(in crate::app) fn handle_finder_key(&mut self, key: KeyEvent) {
        let Some(command) = palette::Command::from_key(key) else {
            return;
        };
        let Some(mut finder) = self.state.take_finder() else {
            return;
        };
        let mut keep_open = true;
        let mut chosen = None;
        match command {
            palette::Command::Insert(c) => finder.insert(c),
            palette::Command::Backspace => finder.backspace(),
            palette::Command::ClearQuery => finder.clear_query(),
            palette::Command::Previous => finder.select_previous(),
            palette::Command::Next => finder.select_next(),
            palette::Command::Cancel => keep_open = false,
            palette::Command::Run => {
                chosen = finder.selected_path();
                keep_open = chosen.is_none();
            }
        }
        if keep_open {
            self.state.open_finder(finder);
        }
        self.needs_redraw = true;
        let Some(path) = chosen else {
            return;
        };
        // Unlike the explorer, the finder is for getting to a file: the editor
        // gets the keyboard.
        self.state.set_focus(Focus::Editor);
        if let Err(error) = self.state.workspace_mut().open_path(&path) {
            self.state.notify_error(error.to_string());
        }
    }
}
