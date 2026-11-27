//! The Errors view: showing it, its keys and its mouse.

use std::time::Instant;

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use super::App;
use crate::app::state::Focus;
use crate::components::errors::{CopyKind, ErrorTarget, ErrorsCommand};
use crate::event::mouse::Clicks;
use crate::ui::ComponentKind;

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
        let rows = self.state.error_rows();
        let errors = self.state.errors_mut();
        errors.reconcile(&rows);
        errors.apply(command, rows.len());
        if command.copy_kind().is_none()
            && !matches!(command, ErrorsCommand::Open | ErrorsCommand::Leave)
        {
            errors.pin(&rows);
        }
        if let Some(kind) = command.copy_kind() {
            self.copy_selected_error(kind);
            return;
        }
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
        let Some(row) = self.state.errors().selected_row(&rows) else {
            return;
        };
        let (target, position) = (row.target.clone(), row.position);
        self.mark_jump();
        match target {
            ErrorTarget::Document(document) => self.state.workspace_mut().show_document(document),
            ErrorTarget::File(path) => {
                if let Err(error) = self.state.workspace_mut().open_path(&path) {
                    self.state.notify_error(error.to_string());
                    return;
                }
            }
        }
        self.state.edit(|editor| editor.go_to(position));
        self.state.set_focus(Focus::Editor);
        self.needs_redraw = true;
    }

    /// Puts the selected problem on the clipboard, as `kind` says. With none selected (the
    /// `[copy]` button is dimmed) nothing happens.
    fn copy_selected_error(&mut self, kind: CopyKind) {
        let rows = self.state.error_rows();
        let Some(text) = self
            .state
            .errors()
            .selected_row(&rows)
            .map(|row| row.copy_text(kind))
        else {
            return;
        };
        self.set_register(crate::clipboard::Register::charwise(text));
        self.state.notify(match kind {
            CopyKind::Full => "copied the problem",
            CopyKind::Message => "copied the problem's message",
            CopyKind::Place => "copied the problem's place",
        });
        self.needs_redraw = true;
    }

    /// Mouse input over the Errors view: a click opens the problem under it (with
    /// ctrl, or as the second click of a double click, it then goes to where the word there is
    /// defined), the wheel scrolls. Returns whether the event was used up.
    pub(super) fn handle_errors_mouse(&mut self, event: MouseEvent) -> bool {
        if self.modal_open()
            || !self.state.is_showing(&ComponentKind::Errors)
            || !self.state.errors().contains(event.column, event.row)
        {
            return false;
        }
        let rows = self.state.error_rows();
        let count = rows.len();
        self.state.errors_mut().reconcile(&rows);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left)
                if self.state.errors().on_copy_button(event.column, event.row) =>
            {
                self.copy_selected_error(CopyKind::Full);
            }
            MouseEventKind::Down(MouseButton::Right) => {
                // The menu is about the problem under the pointer: no problem, no menu.
                if self
                    .state
                    .errors_mut()
                    .select_at(event.column, event.row, count)
                {
                    self.state.errors_mut().pin(&rows);
                    self.state.set_focus(Focus::Errors);
                    self.open_errors_menu(event.column, event.row);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                self.state.set_focus(Focus::Errors);
                if self
                    .state
                    .errors_mut()
                    .select_at(event.column, event.row, count)
                {
                    self.state.errors_mut().pin(&rows);
                    // The first click of a double click opened the problem already.
                    let clicks = self
                        .mouse
                        .register_click(Instant::now(), event.column, event.row);
                    self.open_selected_error();
                    if event.modifiers.contains(KeyModifiers::CONTROL) || clicks == Clicks::Double {
                        self.go_to_definition();
                    }
                }
            }
            MouseEventKind::ScrollUp => self.state.errors_mut().scroll_by(-WHEEL_ROWS, count),
            MouseEventKind::ScrollDown => self.state.errors_mut().scroll_by(WHEEL_ROWS, count),
            _ => return false,
        }
        true
    }
}
