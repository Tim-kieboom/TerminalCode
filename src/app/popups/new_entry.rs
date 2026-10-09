//! Making, and renaming, files and folders from the explorer.

use crossterm::event::KeyEvent;
use std::path::{Path, PathBuf};

use crate::app::App;
use crate::app::state::Focus;
use crate::components::explorer::{ExplorerCommand, NodeKind};
use crate::components::name_prompt::{self, NamePrompt, Purpose};
use crate::components::workspace::DocumentId;
use crate::entries::{self, EntryError, EntryKind};

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

    /// Asks for a new name for the explorer's selected file or folder.
    pub(in crate::app) fn rename_selected(&mut self) {
        let Some(path) = self
            .state
            .explorer()
            .selected_row()
            .map(|row| row.path.clone())
        else {
            return;
        };
        if self.state.explorer().root_path() == Some(path.as_path()) {
            self.state
                .notify("the project folder cannot be renamed here");
            return;
        }
        self.state.open_name_prompt(NamePrompt::rename(path));
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
            name_prompt::Command::Submit => match self.submit_name(&prompt) {
                Ok(()) => return,
                Err(error) => self.state.notify_error(error.to_string()),
            },
        }
        self.state.open_name_prompt(prompt);
    }

    fn submit_name(&mut self, prompt: &NamePrompt) -> Result<(), EntryError> {
        match prompt.purpose() {
            Purpose::Create(kind) => {
                let path = entries::create(prompt.dir(), prompt.name(), *kind)?;
                self.created(path, *kind);
            }
            Purpose::Rename(from) => {
                // Taken before the move, while the files are still where they were.
                let open = self.state.workspace().documents_with_suffix(from);
                let to = entries::rename(from, prompt.name())?;
                self.renamed(from, &to, open);
            }
        }
        Ok(())
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

    /// Brings the open tabs and the tree in step with a file or folder that
    /// now has another path. `open` holds the documents that were inside it and
    /// where in it they were.
    ///
    /// The tabs follow before any event about the old path is handled, so such
    /// a late event names no open file and is harmless.
    fn renamed(&mut self, from: &Path, to: &Path, open: Vec<(DocumentId, PathBuf)>) {
        let moves = open
            .into_iter()
            .map(|(id, suffix)| match suffix.as_os_str().is_empty() {
                // Joining nothing would leave a slash after the name.
                true => (id, to.to_path_buf()),
                false => (id, to.join(suffix)),
            })
            .collect();
        self.state.workspace_mut().set_document_paths(moves);

        let was_open = self
            .state
            .explorer()
            .rows()
            .iter()
            .any(|row| row.path == from && row.expanded);
        let explorer = self.state.explorer_mut();
        let result = explorer
            .refresh_paths(&[from.to_path_buf()])
            .and_then(|()| explorer.reveal(to))
            .and_then(|()| match was_open {
                true => explorer.apply(ExplorerCommand::Expand).map(drop),
                false => Ok(()),
            });
        if let Err(error) = result {
            self.state.notify_error(error.to_string());
        }
    }
}
