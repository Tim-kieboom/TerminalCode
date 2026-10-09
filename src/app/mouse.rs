//! What the mouse does: the editor panes, tabs and the explorer.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use std::time::Instant;

use super::App;
use crate::event::mouse::{ClickTracker, Clicks};

/// Whether the mouse is on, and what counts as a double or triple click.
#[derive(Debug)]
pub(super) struct MouseInput {
    clicks: ClickTracker,
    enabled: bool,
    /// A change the terminal has not been told about yet.
    change: Option<bool>,
}

impl Default for MouseInput {
    fn default() -> Self {
        Self {
            clicks: ClickTracker::default(),
            enabled: true,
            change: None,
        }
    }
}

impl MouseInput {
    fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Switches the mouse on or off, and says which it is now. The terminal
    /// has to be told: see [`MouseInput::take_change`].
    fn toggle(&mut self) -> bool {
        self.enabled = !self.enabled;
        self.change = Some(self.enabled);
        self.enabled
    }

    /// The mouse change the terminal has to be told about, once.
    fn take_change(&mut self) -> Option<bool> {
        self.change.take()
    }

    /// Counts a press at a cell as a single, double or triple click.
    fn register_click(&mut self, now: Instant, column: u16, row: u16) -> Clicks {
        self.clicks.register(now, column, row)
    }
}

impl App {
    /// A mouse change the terminal has to be told about, once.
    pub(crate) fn take_mouse_change(&mut self) -> Option<bool> {
        self.mouse.take_change()
    }

    pub(super) fn toggle_mouse(&mut self) {
        let message = match self.mouse.toggle() {
            true => "mouse on (hold shift to select text in the terminal)",
            false => "mouse off",
        };
        self.state.notify(message);
    }

    pub(super) fn handle_mouse(&mut self, event: MouseEvent) {
        if matches!(event.kind, MouseEventKind::Down(_)) && self.state.dismiss_errors() {
            self.needs_redraw = true;
        }
        if !self.mouse.is_enabled() {
            return;
        }
        if self.handle_menu_mouse(event) {
            self.needs_redraw = true;
            return;
        }
        if self.modal_open() {
            return;
        }
        if self.handle_explorer_mouse(event) {
            self.needs_redraw = true;
            return;
        }
        let extend = event.modifiers.contains(KeyModifiers::SHIFT);
        let sideways = extend;
        let (column, row) = (event.column, event.row);
        let workspace = self.state.workspace_mut();
        let mut closed = None;

        // The wheel scrolls the pane under the pointer; everything else acts
        // on the focused pane (a press also moves focus).
        let result = match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let clicks = self.mouse.register_click(Instant::now(), column, row);
                workspace.mouse_press(column, row, extend, clicks)
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                workspace.with_editor(|editor| editor.mouse_drag(column, row))
            }
            // Middle-clicking a tab closes it.
            MouseEventKind::Down(MouseButton::Middle) => {
                closed = workspace.middle_press(column, row);
                Ok(())
            }
            MouseEventKind::Up(MouseButton::Left) => {
                workspace.with_editor(|editor| editor.mouse_release());
                Ok(())
            }
            MouseEventKind::ScrollUp if sideways => {
                workspace.with_editor_at(column, row, |editor| editor.scroll_columns(-1));
                Ok(())
            }
            MouseEventKind::ScrollDown if sideways => {
                workspace.with_editor_at(column, row, |editor| editor.scroll_columns(1));
                Ok(())
            }
            MouseEventKind::ScrollUp => {
                workspace.with_editor_at(column, row, |editor| editor.scroll_lines(-1));
                Ok(())
            }
            MouseEventKind::ScrollDown => {
                workspace.with_editor_at(column, row, |editor| editor.scroll_lines(1));
                Ok(())
            }
            MouseEventKind::ScrollLeft => {
                workspace.with_editor_at(column, row, |editor| editor.scroll_columns(-1));
                Ok(())
            }
            MouseEventKind::ScrollRight => {
                workspace.with_editor_at(column, row, |editor| editor.scroll_columns(1));
                Ok(())
            }
            _ => return,
        };

        if let Some(result) = closed {
            self.warn_if_unsaved(result);
        }
        if let Err(error) = result {
            self.state.notify_error(error.to_string());
        }
        self.needs_redraw = true;
    }
}

#[cfg(test)]
mod tests;
