//! Editing the open document: clipboard, saving and closing tabs.

use super::App;
use super::keys::NO_FILE_MESSAGE;
use crate::clipboard::{Notice, Register};
use crate::components::editor::Toggled;
use crate::components::workspace::{CloseMany, CloseResult, CloseScope, SaveOutcome};
use crate::error::IdeResult;

impl App {
    pub(super) fn copy(&mut self) -> IdeResult {
        let register = self.state.edit(|editor| editor.copy())?;
        self.set_register(register);
        Ok(())
    }

    pub(super) fn cut(&mut self) -> IdeResult {
        let register = self.state.edit(|editor| editor.cut())?;
        self.set_register(register);
        Ok(())
    }

    /// Fills the clipboard, and says once when the system's could not be
    /// written, so copies are known to stay in the editor.
    pub(super) fn set_register(&mut self, register: Register) {
        if let Some(Notice::SystemUnwritable(error)) = self.clipboard.set(register) {
            self.state.notify(format!(
                "could not write the system clipboard ({error}); only the editor's own copy is used, so paste works here and nowhere else"
            ));
        }
    }

    pub(super) fn paste(&mut self) -> IdeResult {
        let Some(fetched) = self.clipboard.get() else {
            self.state.notify("nothing to paste");
            return Ok(());
        };
        self.state.edit(|editor| editor.paste(&fetched.register))?;
        if fetched.notice == Some(Notice::SystemUnreadable) {
            self.state
                .notify("system clipboard is not readable here; pasted the editor's own copy");
        }
        Ok(())
    }

    /// Text the terminal pasted (bracketed paste, e.g. ctrl+shift+v or a middle
    /// click). Goes straight into the editor; it neither reads nor changes the
    /// clipboard.
    pub(super) fn paste_from_terminal(&mut self, text: String) {
        if self.paste_into_popup(&text) {
            return;
        }
        if self.modal_open() {
            return;
        }
        if self.paste_into_plugin(&text) {
            return;
        }
        if self.state.focus() == crate::app::state::Focus::Terminal {
            self.paste_into_shell(&text);
            return;
        }
        if !self.state.workspace().has_tabs() {
            self.state.notify(NO_FILE_MESSAGE);
            self.needs_redraw = true;
            return;
        }
        let paste = Register::charwise(text);
        if let Err(error) = self.state.edit(|editor| editor.paste(&paste)) {
            self.state.notify_error(error.to_string());
        }
        self.needs_redraw = true;
    }

    /// `ctrl+/`: toggles the comment on the lines of the cursors, and says when the
    /// file has no language to take the marker from.
    pub(super) fn toggle_comment(&mut self) {
        match self.state.edit(|editor| editor.toggle_comment()) {
            Ok(Toggled::NoLanguage) => self.state.notify(
                "no comment marker for this file: its language is not known (see the file's extension)",
            ),
            Ok(_) => {}
            Err(error) => self.state.notify_error(error.to_string()),
        }
    }

    pub(super) fn save(&mut self) -> IdeResult {
        // Saving that formats first is finished when the formatting ends.
        if let Some(document) = self.state.workspace().active_document_id()
            && self.begin_format_save(document)
        {
            return Ok(());
        }
        let outcome = match self.state.workspace_mut().save_active() {
            Ok(outcome) => outcome,
            // Not allowed to write there: the password prompt of a save as root, if it applies.
            Err(error) if self.offer_root_save(&error) => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        match outcome {
            SaveOutcome::Saved => {
                let name = self.state.editor().display_name();
                self.state.notify(format!("saved {name}"));
            }
            SaveOutcome::NeedsConfirmation(name) => {
                self.state.notify_error(overwrite_warning(&name))
            }
            SaveOutcome::FileDeleted(name) => self.state.notify_error(deleted_warning(&name)),
        }
        Ok(())
    }

    pub(super) fn close_tab(&mut self) {
        let result = self.state.workspace_mut().close_tab();
        self.warn_if_unsaved(result);
    }

    pub(super) fn warn_if_unsaved(&mut self, result: CloseResult) {
        if let CloseResult::Unsaved(name) = result {
            self.state.notify(format!(
                "{name} has unsaved changes; close again to discard them"
            ));
        }
    }
}

/// What saving says when the file changed on disk since it was read.
pub(super) fn overwrite_warning(name: &str) -> String {
    format!("{name} changed on disk since you opened it; save again to overwrite it")
}

/// What saving says when the file was deleted since it was read.
pub(super) fn deleted_warning(name: &str) -> String {
    format!("{name} was deleted on disk; save again to recreate it")
}

impl App {
    /// Closes the other tabs of the focused pane, or every tab. Unsaved changes
    /// are asked about first: nothing is closed until the same action is run
    /// again.
    pub(super) fn close_tabs(&mut self, scope: CloseScope) {
        match self.state.workspace_mut().close_tabs(scope) {
            CloseMany::Closed(_) => {}
            CloseMany::Unsaved(names) => self.state.notify(format!(
                "{} would lose unsaved changes; do this again to discard them",
                names.join(", ")
            )),
        }
    }

    /// Opens the file of the tab closed last.
    pub(super) fn reopen_closed_tab(&mut self) {
        match self.state.workspace_mut().reopen_closed_tab() {
            Some(Ok(_)) => {}
            Some(Err(error)) => self.state.notify_error(error.to_string()),
            None => self.state.notify("no closed tab to reopen"),
        }
    }

    /// Saves every modified file, and says what it left alone.
    pub(super) fn save_all(&mut self) {
        // The documents that are formatted first are saved when that ends.
        let dirty = self.state.workspace().dirty_documents();
        let mut formatting = Vec::new();
        for item in dirty {
            if self.begin_format_save(item.document) {
                formatting.push(item.document);
            }
        }
        let report = self.state.workspace_mut().save_all(&formatting);
        for name in &report.changed_on_disk {
            self.state.notify_error(format!(
                "{name} changed on disk since you opened it; save it on its own to overwrite it"
            ));
        }
        for (name, why) in &report.failed {
            self.state
                .notify_error(format!("could not save {name}: {why}"));
        }
        if !report.untitled.is_empty() {
            self.state.notify(format!(
                "{} not saved: it has no file yet",
                report.untitled.join(", ")
            ));
        }
        let skipped = report.changed_on_disk.len() + report.failed.len() + report.untitled.len();
        let message = match (report.saved, skipped) {
            (0, 0) => "nothing to save".to_owned(),
            (1, 0) => "saved 1 file".to_owned(),
            (saved, 0) => format!("saved {saved} files"),
            (0, _) => return,
            (saved, _) => format!("saved {saved} of {} modified files", saved + skipped),
        };
        self.state.notify(message);
    }
}

impl App {
    /// Pins the tab in front to the left of its strip, or lets it go.
    pub(super) fn toggle_pin_tab(&mut self) {
        let pinned = self.state.workspace_mut().toggle_pin_active();
        self.state
            .notify(if pinned { "tab pinned" } else { "tab unpinned" });
    }
}
