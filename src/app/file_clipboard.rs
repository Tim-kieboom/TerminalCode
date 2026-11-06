//! Copying, cutting and pasting files and folders in the explorer. What was copied or cut is
//! kept in the app (the paths, apart from the text clipboard); a paste goes into the selected
//! folder, or the folder of the selected file. A cut and paste is the move of a drag, so open
//! tabs follow the file; a copy runs on a thread of its own when there is an event loop to
//! hear from it, since a big folder takes a while.

use std::path::{Path, PathBuf};

use super::App;
use crate::event::Event;
use crate::files::copy::copy_into;
use crate::files::entries::EntryError;

/// What the explorer holds for a paste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Held {
    path: PathBuf,
    /// Cut: the paste moves it. Otherwise it copies it.
    cut: bool,
}

impl App {
    /// Remembers the explorer's selected entry, for `cut` to move or else to copy.
    pub(super) fn hold_file(&mut self, cut: bool) {
        let explorer = self.state.explorer();
        let Some(row) = explorer.selected_row() else {
            self.state.notify("there is nothing selected to copy");
            return;
        };
        if explorer.root_path() == Some(row.path.as_path()) {
            self.state
                .notify_error("the project folder cannot be copied or cut");
            return;
        }
        let path = row.path.clone();
        let name = name_of(&path);
        self.file_clipboard = Some(Held { path, cut });
        self.state
            .notify(format!("{} {name}", if cut { "cut" } else { "copied" }));
    }

    /// Pastes into the explorer's selected folder, or the folder of the selected file.
    pub(super) fn paste_file(&mut self) {
        let Some(held) = self.file_clipboard.clone() else {
            self.state.notify("nothing was copied or cut");
            return;
        };
        let Some(folder) = self.paste_folder() else {
            self.state.notify("there is nowhere selected to paste into");
            return;
        };
        match held.cut {
            true => self.paste_cut(&held.path, &folder),
            false => self.paste_copy(held.path, folder),
        }
    }

    /// The folder a paste goes into.
    fn paste_folder(&self) -> Option<PathBuf> {
        let row = self.state.explorer().selected_row()?;
        match row.path.is_dir() {
            true => Some(row.path.clone()),
            false => row.path.parent().map(Path::to_path_buf),
        }
    }

    /// Moves what was cut into `folder`, as a drag would. Done at once (a move within a
    /// volume is a rename), and what was cut is not offered again.
    fn paste_cut(&mut self, from: &Path, folder: &Path) {
        if from.parent() == Some(folder) {
            self.state
                .notify(format!("{} is already in that folder", name_of(from)));
            return;
        }
        if !from.exists() {
            self.file_clipboard = None;
            self.state
                .notify_error(format!("{} no longer exists", name_of(from)));
            return;
        }
        self.move_by_drag(from, folder);
        self.file_clipboard = None;
    }

    /// Copies what was copied into `folder`: on a thread when the loop can be told, else here.
    fn paste_copy(&mut self, from: PathBuf, folder: PathBuf) {
        let Some(events) = self.background.sender() else {
            let result = run_copy(&from, &folder);
            self.file_pasted(&from, result);
            return;
        };
        self.state.notify(format!("copying {}...", name_of(&from)));
        std::thread::spawn(move || {
            let result = run_copy(&from, &folder);
            let _ = events.blocking_send(Event::FilePasted { from, result });
        });
    }

    /// A copy is over: shows the copy in the explorer, or says what went wrong.
    pub(super) fn file_pasted(&mut self, from: &Path, result: Result<PathBuf, String>) {
        match result {
            Ok(copy) => {
                if let Err(error) = self.state.explorer_mut().reveal(&copy) {
                    self.state.notify_error(error.to_string());
                }
                self.state
                    .notify(format!("copied {} as {}", name_of(from), name_of(&copy)));
            }
            Err(message) => self.state.notify_error(message),
        }
        self.needs_redraw = true;
    }
}

/// The copy, or the message for the user.
fn run_copy(from: &Path, folder: &Path) -> Result<PathBuf, String> {
    copy_into(from, folder).map_err(|error| match error {
        // A copy that stopped half way leaves what it had made.
        EntryError::Io { .. } if from.is_dir() => {
            format!(
                "could not copy {}: {error} (what was copied is left as it is)",
                name_of(from)
            )
        }
        error => format!("could not copy {}: {error}", name_of(from)),
    })
}

fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}
