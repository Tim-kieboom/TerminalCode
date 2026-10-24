//! The Debug pane: showing it, its keys and its mouse. What it shows is the debug session's
//! (`debug.rs`); choosing a frame or opening a variable asks the adapter, so those go there.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use super::App;
use crate::app::state::Focus;
use crate::components::debug::{Counts, DebugCommand, Section};
use crate::ui::ComponentKind;

/// Rows one wheel notch scrolls.
const WHEEL_ROWS: isize = 3;

impl App {
    /// With the keyboard in the Debug pane, hides it; otherwise shows it and gives it the
    /// keyboard.
    pub(super) fn toggle_debug_pane(&mut self) {
        if self.state.is_showing(&ComponentKind::Debug) && self.state.focus() == Focus::Debug {
            self.state.hide_debug();
            self.state.set_focus(Focus::Editor);
            return;
        }
        self.show_debug_with_keyboard();
    }

    /// Shows the Debug pane and gives it the keyboard, when the layout has one.
    pub(super) fn show_debug_with_keyboard(&mut self) {
        if !self.state.layout().contains(&ComponentKind::Debug) {
            self.state.notify("the layout has no Debug view");
            return;
        }
        self.state.show_debug();
        self.state.set_focus(Focus::Debug);
    }

    pub(super) fn debug_pane_command(&mut self, command: DebugCommand) {
        let counts = Counts::of(self.state.debug());
        self.state.debug_view_mut().apply(command, counts);
        match command {
            DebugCommand::Leave => self.state.set_focus(Focus::Editor),
            DebugCommand::Open | DebugCommand::Expand | DebugCommand::Collapse => {
                self.debug_pane_activate(command);
            }
            DebugCommand::Pin => self.pin_selected_variable(),
            DebugCommand::Remove => self.remove_selected_watch(),
            _ => {}
        }
    }

    /// `enter`, `right` or `left` on the row the selection is on.
    fn debug_pane_activate(&mut self, command: DebugCommand) {
        match self.state.debug_view().section() {
            Section::Stack => {
                if command == DebugCommand::Open {
                    let frame = self.state.debug_view().selected_frame();
                    self.debug_select_frame(frame);
                }
            }
            Section::Variables => {
                let index = self.state.debug_view().selected_variable();
                let expanded = self
                    .state
                    .debug()
                    .tree
                    .rows()
                    .get(index)
                    .map(|row| row.expanded);
                if toggles(command, expanded) {
                    self.debug_toggle_variable(index);
                }
            }
            Section::Watches => {
                let index = self.state.debug_view().selected_watch();
                let expanded = self
                    .state
                    .debug()
                    .watches
                    .rows()
                    .get(index)
                    .map(|row| row.expanded);
                if toggles(command, expanded) {
                    self.debug_watch_toggle(index);
                }
            }
        }
    }

    /// Mouse input over the Debug pane: a click chooses the frame or opens the variable under
    /// it, the wheel scrolls the list it is over. Returns whether the event was used up.
    pub(super) fn handle_debug_mouse(&mut self, event: MouseEvent) -> bool {
        if self.modal_open()
            || !self.state.is_showing(&ComponentKind::Debug)
            || !self.state.debug_view().contains(event.column, event.row)
        {
            return false;
        }
        let counts = Counts::of(self.state.debug());
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.state.set_focus(Focus::Debug);
                let hit = self
                    .state
                    .debug_view_mut()
                    .select_at(event.column, event.row, counts);
                match hit {
                    Some((Section::Stack, index)) => self.debug_select_frame(index),
                    Some((Section::Variables, index)) => self.debug_toggle_variable(index),
                    Some((Section::Watches, index)) => self.debug_watch_toggle(index),
                    None => {}
                }
            }
            MouseEventKind::ScrollUp => {
                self.state
                    .debug_view_mut()
                    .scroll_by(event.column, event.row, -WHEEL_ROWS, counts)
            }
            MouseEventKind::ScrollDown => {
                self.state
                    .debug_view_mut()
                    .scroll_by(event.column, event.row, WHEEL_ROWS, counts)
            }
            _ => return false,
        }
        true
    }
}

/// Whether `command` opens or closes a row that is `expanded` (or is not a row, `None`).
fn toggles(command: DebugCommand, expanded: Option<bool>) -> bool {
    matches!(
        (command, expanded),
        (DebugCommand::Open, Some(_))
            | (DebugCommand::Expand, Some(false))
            | (DebugCommand::Collapse, Some(true))
    )
}

impl App {
    /// A click on a button of the toolbar over the editor does what the button says; the rest of
    /// the toolbar takes the mouse too (a click there is not for the text under it). Whether the
    /// event was used up.
    pub(super) fn handle_debug_toolbar_mouse(&mut self, event: MouseEvent) -> bool {
        let toolbar = self.state.debug_toolbar();
        if self.modal_open() || !toolbar.contains(event.column, event.row) {
            return false;
        }
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(button) = toolbar.button_at(event.column, event.row) {
                    self.run_action(button.action());
                }
            }
            MouseEventKind::Moved => return false,
            _ => {}
        }
        self.needs_redraw = true;
        true
    }
}
