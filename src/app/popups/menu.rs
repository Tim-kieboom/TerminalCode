//! The context menu on an explorer row. Its items run the same actions as the
//! keys do, on the path the menu was opened on.

use std::fs;
use std::path::Path;

use crossterm::event::KeyEvent;

use crate::app::App;
use crate::components::menu::{self, Entry, Menu};
use crate::event::action::Action;

impl App {
    /// Opens the menu on the explorer's selected row, just below it.
    pub(in crate::app) fn open_context_menu(&mut self) {
        let explorer = self.state.explorer();
        let Some(row) = explorer.selected_row() else {
            self.state.notify("there is nothing here to open a menu on");
            return;
        };
        let target = row.path.clone();
        let is_root = explorer.root_path() == Some(target.as_path());
        let anchor = explorer.selected_cell();
        self.state
            .open_menu(Menu::new(explorer_entries(is_root), anchor, target));
    }

    pub(in crate::app) fn handle_menu_key(&mut self, key: KeyEvent) {
        let Some(command) = menu::Command::from_key(key) else {
            return;
        };
        let Some(mut menu) = self.state.take_menu() else {
            return;
        };
        self.needs_redraw = true;
        match command {
            menu::Command::Previous => menu.select_previous(),
            menu::Command::Next => menu.select_next(),
            menu::Command::Cancel => return,
            menu::Command::Run => {
                if let Some(action) = menu.selected_action().cloned() {
                    self.run_on_target(menu.target(), action);
                }
                return;
            }
        }
        self.state.open_menu(menu);
    }

    /// Runs `action` on `target`, which the menu was opened on. The actions
    /// work on the explorer's selection, so the target is selected first; if
    /// it is gone, or no longer in the tree, nothing runs, and the selection
    /// never stands in for it.
    fn run_on_target(&mut self, target: &Path, action: Action) {
        if fs::symlink_metadata(target).is_err() {
            self.state
                .notify_error(format!("{} no longer exists", name_of(target)));
            return;
        }
        if let Err(error) = self.state.explorer_mut().reveal(target) {
            self.state.notify_error(error.to_string());
        }
        let selected = self.state.explorer().selected_row();
        if selected.map(|row| row.path.as_path()) != Some(target) {
            self.state
                .notify_error(format!("{} is no longer in the tree", name_of(target)));
            return;
        }
        self.run_action(action);
    }
}

/// What the menu on an explorer row offers. The project folder itself cannot
/// be renamed or deleted, so it only gets the entries that add to it.
fn explorer_entries(is_root: bool) -> Vec<Entry> {
    let mut entries = vec![
        Entry::Item {
            label: "New File",
            action: Action::CreateFile,
        },
        Entry::Item {
            label: "New Folder",
            action: Action::CreateFolder,
        },
    ];
    if !is_root {
        entries.extend([
            Entry::Item {
                label: "Rename",
                action: Action::Rename,
            },
            Entry::Separator,
            Entry::Item {
                label: "Delete",
                action: Action::DeleteSelected,
            },
        ]);
    }
    entries
}

fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}
