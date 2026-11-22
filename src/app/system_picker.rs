//! The operating system's own file dialog for the Open File and Open Folder prompts (`ctrl+e`):
//! the Explorer on Windows, Finder on macOS, the desktop's portal on Linux. It is on a thread of
//! its own (it holds the thread until it is closed) and the path it gives comes back as
//! `Event::SystemPick`; the editor keeps running meanwhile.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::App;
use super::open::expand_path;
use crate::components::popups::name_prompt::{NamePrompt, Purpose};
use crate::event::Event;

/// A dialog that is gone sooner than this without a path did not open.
const TOO_QUICK_TO_BE_A_PERSON: Duration = Duration::from_millis(500);

/// Why a dialog may not open, for the message.
const WHY_NOT: &str = match cfg!(target_os = "linux") {
    true => "it needs the desktop portal (libdbus-1 the program can load) or `zenity` on the PATH",
    false => "the system gave no dialog",
};

impl App {
    /// Opens the dialog at the folder typed so far (or the prompt's own folder), for a file or a
    /// folder as the prompt asks. Prompts with no such dialog do nothing.
    pub(super) fn open_system_dialog(&mut self, prompt: &mut NamePrompt) {
        if !prompt.can_browse() {
            return;
        }
        let Some(events) = self.background.sender() else {
            self.state
                .notify("the system's file dialog needs the running editor");
            return;
        };
        if !prompt.begin_dialog() {
            self.state
                .notify("the system's file dialog is open already");
            return;
        }
        let typed = expand_path(prompt.typed_folder(), prompt.dir());
        let start = match typed.is_dir() {
            true => typed,
            false => prompt.dir().to_path_buf(),
        };
        let folder = *prompt.purpose() == Purpose::OpenFolder;
        std::thread::spawn(move || {
            let began = Instant::now();
            let path = pick(&start, folder);
            // The crate gives the same `None` for a cancel and for a dialog that could not open
            // (it only logs why), but nobody cancels in a fraction of a second.
            let failed = path.is_none() && began.elapsed() < TOO_QUICK_TO_BE_A_PERSON;
            let _ = events.blocking_send(Event::SystemPick { path, failed });
        });
    }

    /// The dialog was closed: a path chosen is the name, and is opened as if it had been typed
    /// (and `enter` pressed); a cancel leaves the prompt as it was. A dialog that could not open
    /// at all (no desktop portal, no `zenity`) is told apart by being too quick for a person: it says so.
    pub(super) fn system_pick_done(&mut self, path: Option<PathBuf>, failed: bool) {
        let Some(mut prompt) = self.state.take_name_prompt() else {
            return;
        };
        self.needs_redraw = true;
        if !prompt.end_dialog() {
            return self.state.open_name_prompt(prompt);
        }
        if failed {
            self.state
                .notify_error(format!("the system's file dialog did not open: {WHY_NOT}"));
        }
        match path {
            Some(path) => {
                prompt.stop_browsing();
                prompt.set_path(&path);
                self.finish_name(prompt, true);
            }
            None => self.state.open_name_prompt(prompt),
        }
    }
}

/// Shows the dialog, starting in `start`, and waits for it.
fn pick(start: &Path, folder: bool) -> Option<PathBuf> {
    let dialog = rfd::FileDialog::new().set_directory(start);
    match folder {
        true => dialog.set_title("Open folder").pick_folder(),
        false => dialog.set_title("Open file").pick_file(),
    }
}
