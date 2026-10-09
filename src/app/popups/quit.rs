//! Quitting, and the prompt that asks about unsaved changes first.

use crossterm::event::KeyEvent;

use crate::app::editing::overwrite_warning;
use crate::app::{App, Flow};
use crate::components::quit_prompt::{self, QuitPrompt};
use crate::components::workspace::{DocumentId, SaveOutcome};

impl App {
    /// Quits, after asking what to do with documents that have unsaved
    /// changes.
    pub(in crate::app) fn quit(&mut self) {
        let dirty = self.state.workspace().dirty_documents();
        if dirty.is_empty() {
            self.flow = Flow::Quit;
        } else {
            self.state.open_quit_prompt(QuitPrompt::new(dirty));
        }
    }

    pub(in crate::app) fn handle_prompt_key(&mut self, key: KeyEvent) {
        let Some(command) = quit_prompt::Command::from_key(key) else {
            return;
        };
        let Some(mut prompt) = self.state.take_quit_prompt() else {
            return;
        };

        let mut keep_open = true;
        match command {
            quit_prompt::Command::Previous => prompt.select_previous(),
            quit_prompt::Command::Next => prompt.select_next(),
            quit_prompt::Command::Cancel => keep_open = false,
            quit_prompt::Command::Discard => prompt.remove_selected(),
            quit_prompt::Command::DiscardAll => self.flow = Flow::Quit,
            quit_prompt::Command::Save => {
                if let Some(item) = prompt.selected_item().cloned() {
                    self.save_for_prompt(&mut prompt, &item.name, item.document);
                }
            }
            quit_prompt::Command::SaveAll => {
                while let Some(item) = prompt.selected_item().cloned() {
                    if !self.save_for_prompt(&mut prompt, &item.name, item.document) {
                        break;
                    }
                }
            }
        }
        if prompt.is_empty() {
            self.flow = Flow::Quit;
        } else if keep_open && self.flow != Flow::Quit {
            self.state.open_quit_prompt(prompt);
        }
        self.needs_redraw = true;
    }

    /// Saves one document of the prompt and drops it from the list. On
    /// failure it stays listed and the error is shown.
    fn save_for_prompt(&mut self, prompt: &mut QuitPrompt, name: &str, id: DocumentId) -> bool {
        match self.state.workspace_mut().save_document(id) {
            Ok(SaveOutcome::Saved) => {
                prompt.remove_selected();
                true
            }
            Ok(SaveOutcome::NeedsConfirmation(name)) => {
                self.state.notify_error(overwrite_warning(&name));
                false
            }
            Err(error) => {
                self.state.notify_error(format!("{name}: {error}"));
                false
            }
        }
    }
}
