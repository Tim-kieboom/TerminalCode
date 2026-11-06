//! The list under the Open File and Open Folder prompts: the entries of the folder typed so far.
//! The folder is read on a thread of its own when there is an event loop (a slow or huge folder
//! does not hold up the keys); the answer is matched against the name being typed (see
//! `components/popups/name_prompt/picker.rs`).

use std::path::PathBuf;

use super::App;
use super::open::expand_path;
use crate::components::popups::name_prompt::NamePrompt;
use crate::components::popups::path_list::{self, Entry};
use crate::event::Event;

impl App {
    /// Shows a path prompt with the list of the folder it starts in.
    pub(super) fn open_path_prompt(&mut self, mut prompt: NamePrompt) {
        self.sync_path_prompt(&mut prompt);
        self.state.open_name_prompt(prompt);
    }

    /// Reads the folder the name has got to, when it is not the one the list is of, and
    /// matches the list against the name.
    pub(super) fn sync_path_prompt(&mut self, prompt: &mut NamePrompt) {
        if !prompt.has_list() {
            return;
        }
        let dir = expand_path(prompt.typed_folder(), prompt.dir());
        if prompt.needs_listing_of(&dir) {
            let request = prompt.begin_listing(dir.clone());
            self.read_folder(prompt, request, dir);
        }
        prompt.refresh_matches();
    }

    /// Reads `dir` on a thread, or here when nothing could hear from the thread.
    fn read_folder(&mut self, prompt: &mut NamePrompt, request: u64, dir: PathBuf) {
        let Some(events) = self.background.sender() else {
            let result = path_list::read_dir(&dir);
            prompt.finish_listing(request, dir, result);
            return;
        };
        std::thread::spawn(move || {
            let result = path_list::read_dir(&dir);
            let _ = events.blocking_send(Event::PathListing {
                request,
                dir,
                result,
            });
        });
    }

    /// A folder was read: the list shows it, when the prompt is still there and still waits
    /// for it.
    pub(super) fn path_listing_ready(
        &mut self,
        request: u64,
        dir: PathBuf,
        result: Result<Vec<Entry>, String>,
    ) {
        let Some(mut prompt) = self.state.take_name_prompt() else {
            return;
        };
        if prompt.finish_listing(request, dir, result) {
            self.needs_redraw = true;
        }
        self.state.open_name_prompt(prompt);
    }
}
