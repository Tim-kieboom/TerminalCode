//! Deleting the explorer's selection: to the OS trash, with questions first
//! when that would lose unsaved changes or when the trash is not available.

use crossterm::event::KeyEvent;
use std::path::{Path, PathBuf};

use crate::app::App;
use crate::components::confirm::{Answer, Confirm, Purpose};
use crate::removal::{self, TrashFn};

/// How the app takes files away; tests swap the trash for a stand-in.
#[derive(Debug)]
pub(in crate::app) struct Removal {
    trash: TrashFn,
}

impl Default for Removal {
    fn default() -> Self {
        Self {
            trash: removal::move_to_trash,
        }
    }
}

impl Removal {
    #[cfg(test)]
    pub(in crate::app) fn with_trash(trash: TrashFn) -> Self {
        Self { trash }
    }
}

impl App {
    /// Deletes the explorer's selected file or folder.
    pub(in crate::app) fn delete_selected(&mut self) {
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
                .notify("the project folder cannot be deleted here");
            return;
        }
        let affected = self.state.workspace().documents_under(&path);
        let unsaved = self.state.workspace().dirty_among(&affected);
        if unsaved.is_empty() {
            self.trash(&path);
            return;
        }
        let names: Vec<String> = unsaved.into_iter().map(|item| item.name).collect();
        self.state
            .open_confirm(Confirm::discard_and_delete(&path, &names));
    }

    pub(in crate::app) fn handle_confirm_key(&mut self, key: KeyEvent) {
        let Some(answer) = Answer::from_key(key) else {
            return;
        };
        let Some(confirm) = self.state.take_confirm() else {
            return;
        };
        self.needs_redraw = true;
        if answer != Answer::Yes {
            return;
        }
        match confirm.purpose() {
            Purpose::DiscardAndDelete(path) => self.trash(path),
            Purpose::DeletePermanently(path) => self.delete_for_good(path),
        }
    }

    /// Moves `path` to the trash; when that is not possible, asks whether to
    /// delete it for good.
    fn trash(&mut self, path: &Path) {
        let affected = self.state.workspace().documents_under(path);
        match (self.removal.trash)(path) {
            Ok(()) => {
                self.deleted(path, &affected);
                self.state
                    .notify(format!("moved {} to the trash", name_of(path)));
            }
            Err(reason) => self
                .state
                .open_confirm(Confirm::delete_permanently(path, &reason)),
        }
    }

    fn delete_for_good(&mut self, path: &Path) {
        let affected = self.state.workspace().documents_under(path);
        match removal::remove_permanently(path) {
            Ok(()) => {
                self.deleted(path, &affected);
                self.state.notify(format!("deleted {}", name_of(path)));
            }
            Err(error) => self
                .state
                .notify_error(format!("{}: {error}", name_of(path))),
        }
    }

    /// Brings the screen in line with a file or folder that is gone.
    fn deleted(&mut self, path: &Path, affected: &[crate::components::workspace::DocumentId]) {
        self.state.workspace_mut().close_documents(affected);
        let parent: Vec<PathBuf> = path.parent().map(Path::to_path_buf).into_iter().collect();
        if let Err(error) = self.state.explorer_mut().refresh_paths(&parent) {
            self.state.notify_error(error.to_string());
        }
    }
}

fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}
