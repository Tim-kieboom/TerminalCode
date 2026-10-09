//! Making a new file or folder from the explorer.

use crossterm::event::KeyEvent;
use std::path::PathBuf;

use crate::app::App;
use crate::app::state::Focus;
use crate::components::explorer::NodeKind;
use crate::components::name_prompt::{self, NamePrompt};
use crate::entries::{self, EntryKind};

impl App {
    /// Asks for the name of a new entry in the folder the explorer has
    /// selected (or holds the selected file), or in the project root.
    pub(in crate::app) fn create_entry(&mut self, kind: EntryKind) {
        let explorer = self.state.explorer();
        let dir = match explorer.selected_row() {
            Some(row) if row.kind == NodeKind::Dir => Some(row.path.clone()),
            Some(row) => row.path.parent().map(PathBuf::from),
            None => None,
        };
        let Some(dir) = dir.or_else(|| explorer.root_path().map(PathBuf::from)) else {
            self.state.notify("there is no project to add files to");
            return;
        };
        self.state.open_name_prompt(NamePrompt::new(kind, dir));
    }

    pub(in crate::app) fn handle_name_key(&mut self, key: KeyEvent) {
        let Some(command) = name_prompt::Command::from_key(key) else {
            return;
        };
        let Some(mut prompt) = self.state.take_name_prompt() else {
            return;
        };
        self.needs_redraw = true;
        match command {
            name_prompt::Command::Insert(c) => prompt.insert(c),
            name_prompt::Command::Backspace => prompt.backspace(),
            name_prompt::Command::ClearName => prompt.clear(),
            name_prompt::Command::Cancel => return,
            name_prompt::Command::Submit => {
                match entries::create(prompt.dir(), prompt.name(), prompt.kind()) {
                    Ok(path) => return self.created(path, prompt.kind()),
                    Err(error) => self.state.notify_error(error.to_string()),
                }
            }
        }
        self.state.open_name_prompt(prompt);
    }

    /// Shows what was just made; a new file is opened for editing.
    fn created(&mut self, path: PathBuf, kind: EntryKind) {
        if let Err(error) = self.state.explorer_mut().reveal(&path) {
            self.state.notify_error(error.to_string());
        }
        if kind == EntryKind::File {
            self.state.set_focus(Focus::Editor);
            if let Err(error) = self.state.workspace_mut().open_path(&path) {
                self.state.notify_error(error.to_string());
            }
        }
    }
}
