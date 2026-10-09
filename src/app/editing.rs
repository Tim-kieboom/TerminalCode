//! Editing the open document: clipboard, saving and closing tabs.

use super::App;
use super::keys::NO_FILE_MESSAGE;
use crate::clipboard::{Notice, Register};
use crate::components::workspace::{CloseResult, SaveOutcome};
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
