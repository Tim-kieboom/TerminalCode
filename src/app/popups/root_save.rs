//! Save as root (Linux): a save that fails because the user may not write the
//! file or its folder asks for the password and has `sudo` put the text in
//! place ([`RootWriter`]). The password is typed masked, handed to `sudo` on a
//! thread of its own, wiped, and never kept: every such save asks again.
//! See "Save as root" in `docs/todo.md`.

use std::path::PathBuf;

use crossterm::event::KeyEvent;

use crate::app::App;
use crate::buffer::FileError;
use crate::components::popups::root_prompt::{self, RootPrompt};
use crate::event::Event;
use crate::files::rootsave::{RootError, RootWriter};
use crate::ids::DocumentId;

/// What a save as root in progress is writing.
#[derive(Debug)]
pub(in crate::app) struct RootSave {
    document: DocumentId,
    path: PathBuf,
    text: String,
    /// The version of the text, so that a buffer edited meanwhile stays modified.
    version: u64,
}

/// What the app keeps for saving as root.
#[derive(Debug, Default)]
pub(in crate::app) struct Root {
    save: Option<RootSave>,
    writer: RootWriter,
    /// `sudo` is missing or refuses this user: there is no point in asking again.
    unavailable: Option<String>,
}

impl App {
    /// Uses `program` instead of `sudo`, staging the text in `folder` (for the tests).
    #[cfg(test)]
    pub(crate) fn with_root_writer(mut self, writer: RootWriter) -> Self {
        self.root.writer = writer;
        self
    }

    /// A save failed with `error`: when it is because the user may not write the
    /// file, the password prompt opens (on Linux, for a file the app did not lock
    /// on purpose). Whether it took the error over.
    pub(in crate::app) fn offer_root_save(&mut self, error: &FileError) -> bool {
        if !cfg!(target_os = "linux") || !refused_for_permission(error) {
            return false;
        }
        let Some(document) = self.state.workspace().active_document_id() else {
            return false;
        };
        let Some((path, text, version, locked)) = self.state.workspace().root_save_source(document)
        else {
            return false;
        };
        if locked || self.root.save.is_some() {
            return false;
        }
        if let Some(message) = &self.root.unavailable {
            self.state.notify_error(message.clone());
            return true;
        }
        self.state
            .open_root_prompt(RootPrompt::new(&path, document));
        self.root.save = Some(RootSave {
            document,
            path,
            text,
            version,
        });
        self.needs_redraw = true;
        true
    }

    pub(in crate::app) fn handle_root_key(&mut self, key: KeyEvent) {
        let Some(command) = root_prompt::Command::from_key(key) else {
            return;
        };
        let Some(prompt) = self.state.root_prompt_mut() else {
            return;
        };
        self.needs_redraw = true;
        // While `sudo` works the keys do nothing: the answer decides what is next.
        if prompt.is_waiting() {
            return;
        }
        match command {
            root_prompt::Command::Insert(c) => prompt.insert(c),
            root_prompt::Command::Backspace => prompt.backspace(),
            root_prompt::Command::Clear => prompt.clear(),
            root_prompt::Command::Cancel => {
                self.state.close_root_prompt();
                self.root.save = None;
                self.state.notify("not saved");
            }
            root_prompt::Command::Submit => self.submit_root_password(),
        }
    }

    /// Gives the password to `sudo`, which runs on a thread so that nothing waits
    /// for it (a wrong password makes it pause).
    fn submit_root_password(&mut self) {
        let Some(prompt) = self.state.root_prompt_mut() else {
            return;
        };
        if prompt.password_is_empty() {
            return;
        }
        let Some(save) = &self.root.save else {
            return;
        };
        let password = prompt.take_password();
        let writer = self.root.writer.clone();
        let (path, text) = (save.path.clone(), save.text.clone());
        let sender = self.background.sender();
        std::thread::spawn(move || {
            let result = writer.write(&password, &path, &text);
            // Wiped before anything else happens.
            drop(password);
            if let Some(sender) = sender {
                let _ = sender.blocking_send(Event::RootSaved(result));
            }
        });
    }

    /// `sudo` has answered.
    pub(in crate::app) fn root_save_finished(&mut self, result: Result<(), RootError>) {
        self.needs_redraw = true;
        let Some(save) = self.root.save.take() else {
            return;
        };
        match result {
            Ok(()) => {
                self.state.close_root_prompt();
                self.state.workspace_mut().mark_written_as_root(
                    save.document,
                    &save.text,
                    save.version,
                );
                self.state
                    .notify(format!("saved {} as root", save.path.display()));
            }
            Err(RootError::WrongPassword) => {
                // The same prompt, asking again, until Esc.
                self.root.save = Some(save);
                if let Some(prompt) = self.state.root_prompt_mut() {
                    prompt.rejected("wrong password, try again");
                }
            }
            Err(error) => {
                self.state.close_root_prompt();
                if matches!(error, RootError::NoSudo | RootError::NotAllowed(_)) {
                    self.root.unavailable = Some(error.message());
                }
                self.state.notify_error(error.message());
            }
        }
    }

    /// `File: Edit Anyway`: lets the text of a file that is read-only to the user
    /// be edited, which saving then needs root for.
    pub(in crate::app) fn unlock_for_root(&mut self) {
        if !cfg!(target_os = "linux") {
            self.state.notify("saving as root is only built for Linux");
            return;
        }
        if self.state.workspace_mut().unlock_active_for_root() {
            self.state
                .notify("editable: saving it asks for the root password");
        } else {
            self.state
                .notify("this file stays read-only: the app locked it on purpose");
        }
    }
}

/// Whether a save failed because the user is not allowed to write there.
pub(in crate::app) fn refused_for_permission(error: &FileError) -> bool {
    match error {
        FileError::ReadOnly { .. } => true,
        FileError::Io { source, .. } => source.kind() == std::io::ErrorKind::PermissionDenied,
        _ => false,
    }
}
