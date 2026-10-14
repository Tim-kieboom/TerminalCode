//! The Errors view: showing it, its keys and its mouse.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use super::App;
use crate::app::state::Focus;
use crate::components::ComponentKind;
use crate::components::errors::ErrorsCommand;

/// Rows one wheel notch scrolls.
const WHEEL_ROWS: isize = 3;

impl App {
    /// With the keyboard in the Errors view, hides it; otherwise shows it and
    /// gives it the keyboard.
    pub(super) fn toggle_errors(&mut self) {
        if self.state.is_showing(&ComponentKind::Errors) && self.state.focus() == Focus::Errors {
            self.state.hide_errors();
            self.state.set_focus(Focus::Editor);
            return;
        }
        self.show_errors_with_keyboard();
    }

    /// Shows the Errors view and gives it the keyboard, when the layout has one.
    pub(super) fn show_errors_with_keyboard(&mut self) {
        if !self.state.layout().contains(&ComponentKind::Errors) {
            self.state.notify("the layout has no Errors view");
            return;
        }
        self.state.show_errors();
        self.state.set_focus(Focus::Errors);
    }

    pub(super) fn errors_command(&mut self, command: ErrorsCommand) {
        let count = self.state.error_rows().len();
        self.state.errors_mut().apply(command, count);
        match command {
            ErrorsCommand::Open => self.open_selected_error(),
            ErrorsCommand::Leave => self.state.set_focus(Focus::Editor),
            _ => {}
        }
    }

    /// Opens the file of the selected problem with the cursor on it, and gives the
    /// keyboard to the editor.
    fn open_selected_error(&mut self) {
        let rows = self.state.error_rows();
        let Some(row) = rows.get(self.state.errors().selected()) else {
            return;
        };
        let (document, position) = (row.document, row.position);
        self.mark_jump();
        self.state.workspace_mut().show_document(document);
        self.state.edit(|editor| editor.go_to(position));
        self.state.set_focus(Focus::Editor);
        self.needs_redraw = true;
    }

    /// Mouse input over the Errors view: a click opens the problem under it, the
    /// wheel scrolls. Returns whether the event was used up.
    pub(super) fn handle_errors_mouse(&mut self, event: MouseEvent) -> bool {
        if self.modal_open()
            || !self.state.is_showing(&ComponentKind::Errors)
            || !self.state.errors().contains(event.column, event.row)
        {
            return false;
        }
        let count = self.state.error_rows().len();
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.state.set_focus(Focus::Errors);
                if self
                    .state
                    .errors_mut()
                    .select_at(event.column, event.row, count)
                {
                    self.open_selected_error();
                }
            }
            MouseEventKind::ScrollUp => self.state.errors_mut().scroll_by(-WHEEL_ROWS, count),
            MouseEventKind::ScrollDown => self.state.errors_mut().scroll_by(WHEEL_ROWS, count),
            _ => return false,
        }
        true
    }
}
