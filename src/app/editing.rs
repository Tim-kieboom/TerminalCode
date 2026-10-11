//! Editing the open document: clipboard, saving and closing tabs.

use super::App;
use super::keys::NO_FILE_MESSAGE;
use crate::clipboard::{Notice, Register};
use crate::components::workspace::{CloseMany, CloseResult, CloseScope, SaveOutcome};
use crate::error::IdeResult;

impl App {
    pub(super) fn copy(&mut self) -> IdeResult {
        let register = self.state.edit(|editor| editor.copy())?;
        self.clipboard.set(register);
        Ok(())
    }

    pub(super) fn cut(&mut self) -> IdeResult {
        let register = self.state.edit(|editor| editor.cut())?;
        self.clipboard.set(register);
        Ok(())
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

    pub(super) fn save(&mut self) -> IdeResult {
        match self.state.workspace_mut().save_active()? {
            SaveOutcome::Saved => {
                let name = self.state.editor().display_name();
                self.state.notify(format!("saved {name}"));
            }
            SaveOutcome::NeedsConfirmation(name) => {
                self.state.notify_error(overwrite_warning(&name))
            }
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
        let report = self.state.workspace_mut().save_all();
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
