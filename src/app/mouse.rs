//! What the mouse does: the editor panes, tabs and the explorer.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use std::time::Instant;

use super::App;
use super::state::Popup;
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

    /// Makes the mouse `enabled`, telling the terminal if that changes anything.
    pub(super) fn set(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            self.change = Some(enabled);
        }
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
    pub(super) fn register_click(&mut self, now: Instant, column: u16, row: u16) -> Clicks {
        self.clicks.register(now, column, row)
    }
}

/// How many lines one notch of the wheel is, as a plugin is told.
const WHEEL_LINES: i16 = 3;

impl App {
    /// A mouse change the terminal has to be told about, once.
    pub(crate) fn take_mouse_change(&mut self) -> Option<bool> {
        self.mouse.take_change()
    }

    /// A middle click on the text: the cursor goes there and the clipboard is
    /// pasted, as many terminals do with their primary selection.
    fn paste_at(&mut self, column: u16, row: u16) {
        match self.state.workspace_mut().place_cursor_at(column, row) {
            Ok(true) => {}
            Ok(false) => return,
            Err(error) => return self.state.notify_error(error.to_string()),
        }
        self.state.set_focus(crate::app::state::Focus::Editor);
        if let Err(error) = self.paste() {
            self.state.notify_error(error.to_string());
        }
    }

    pub(super) fn toggle_mouse(&mut self) {
        let message = match self.mouse.toggle() {
            true => "mouse on (hold shift to select text in the terminal)",
            false => "mouse off",
        };

        self.state.notify(message);
    }

    pub(super) fn handle_mouse(&mut self, event: MouseEvent) {
        if self.mouse_overlays(event) {
            return;
        }

        if matches!(event.kind, MouseEventKind::Down(_)) && self.state.dismiss_errors() {
            self.needs_redraw = true;
        }

        // A click puts the cursor elsewhere: the word is over.
        if matches!(event.kind, MouseEventKind::Down(_)) {
            self.close_completion();
            // A click anywhere takes the keys back from the find bar; it stays open.
            self.state.set_find_focused(false);
        }

        let wheel = is_wheel(event.kind);
        if wheel && self.wheel_in_box(event) {
            return;
        }

        // A click, a press or the wheel takes what the server said away (a box
        // pinned with `f6` only a click does); moving the pointer is what it follows.
        let pinned = self
            .state
            .hover_box()
            .is_some_and(|shown| shown.is_pinned());
        if event.kind != MouseEventKind::Moved && !(wheel && pinned) {
            self.close_hover();
        }

        if !self.mouse.is_enabled() {
            return;
        }

        // In this order: what is on top of something else comes first. A stage that took
        // the event ends it.
        for stage in PANE_STAGES {
            if stage(self, event) {
                self.needs_redraw = true;
                return;
            }
        }

        // A click anywhere closes the box with the diagnostics, like a key does.
        if matches!(event.kind, MouseEventKind::Down(_)) && self.state.diagnostic_popup().is_some()
        {
            self.state.close_diagnostic_popup();
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

        if self.editor_pointer(event) {
            return;
        }
        self.editor_pane_mouse(event);
    }

    /// What is drawn over everything: a message, the debugger's buttons and its box.
    /// Returns whether one of them took the event.
    fn mouse_overlays(&mut self, event: MouseEvent) -> bool {
        // A click on a message takes that message away and goes no further.
        if event.kind == MouseEventKind::Down(MouseButton::Left)
            && self.state.click_notification(event.column, event.row)
        {
            self.needs_redraw = true;
            return true;
        }
        // The buttons of the debugger over the editor.
        if self.handle_debug_toolbar_mouse(event) {
            return true;
        }
        // A click or the wheel on the box of the debugger is for the box.
        self.handle_debug_hover_mouse(event)
    }

    /// The wheel over a box of text that is longer than it shows scrolls it, and over a
    /// plugin's pane it is told to the plugin, which scrolls what it shows. Returns
    /// whether it did.
    fn wheel_in_box(&mut self, event: MouseEvent) -> bool {
        let down = event.kind == MouseEventKind::ScrollDown;
        if self.scroll_docs_under(event.column, event.row, down) {
            return true;
        }
        if !self.popup_allows_plugin_input() {
            return false;
        }
        let Some(view) = self.state.plugin_at(event.column, event.row) else {
            return false;
        };
        let lines = match down {
            true => WHEEL_LINES,
            false => -WHEEL_LINES,
        };
        self.send_to_plugin(view.as_str(), crate::plugin_api::ToPlugin::Scroll { lines });
        true
    }

    /// Whether the mouse may reach a plugin's pane: no popup is open, or it is a plugin's.
    fn popup_allows_plugin_input(&self) -> bool {
        matches!(self.state.popup(), Popup::None | Popup::PluginView(_))
    }

    /// Clicks on a plugin's panes and popup. Returns whether one took the event.
    fn handle_plugin_mouse(&mut self, event: MouseEvent) -> bool {
        let (column, row) = (event.column, event.row);
        let allowed = self.popup_allows_plugin_input();
        match event.kind {
            // A click on a plugin's pane gives it the keyboard.
            MouseEventKind::Down(MouseButton::Left) => {
                (allowed && self.click_in_plugin_pane(column, row))
                    // A click outside the popup of a plugin closes it.
                    || self.click_outside_plugin_popup(column, row)
            }
            // A right click on a plugin's rows asks the plugin for a menu.
            MouseEventKind::Down(MouseButton::Right) => {
                allowed && self.right_click_in_plugin_pane(column, row)
            }
            _ => false,
        }
    }

    /// The pointer over a word, a click with `ctrl` on it, a right click and a click on the
    /// gutter's markers. Returns whether one took the event.
    fn editor_pointer(&mut self, event: MouseEvent) -> bool {
        let (column, row) = (event.column, event.row);
        match event.kind {
            // The pointer over a word waits, then asks the server about it.
            MouseEventKind::Moved => {
                self.hover_pointer_moved(column, row);
                true
            }
            // Ctrl and a left press on a word: where it is defined. The cursor stays.
            MouseEventKind::Down(MouseButton::Left)
                if event.modifiers.contains(KeyModifiers::CONTROL)
                    && self.state.workspace().text_at_cell(column, row).is_some() =>
            {
                self.go_to_definition_at(column, row);
                self.needs_redraw = true;
                true
            }
            MouseEventKind::Down(MouseButton::Right) => {
                // A right click on a tab opens the menu of the tab; on the text, the
                // editor's menu; elsewhere it does nothing.
                if !self.right_click_tab(column, row) {
                    self.right_click_editor(column, row);
                }
                self.needs_redraw = true;
                true
            }
            // A click on the marker column of the gutter sets or takes away a breakpoint.
            MouseEventKind::Down(MouseButton::Left) if !adds_cursor(event.modifiers) => {
                let marker = self.state.workspace().gutter_marker_at(column, row);
                let Some((document, line)) = marker else {
                    return false;
                };
                self.toggle_breakpoint_in(document, line);
                true
            }
            _ => false,
        }
    }

    /// Presses, drags and the wheel on the editor panes.
    fn editor_pane_mouse(&mut self, event: MouseEvent) {
        let extend = event.modifiers.contains(KeyModifiers::SHIFT);
        let (column, row) = (event.column, event.row);
        let workspace = self.state.workspace_mut();
        let mut closed = None;
        let mut paste_at = None;

        // The wheel scrolls the pane under the pointer; everything else acts
        // on the focused pane (a press also moves focus).
        let result = match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let clicks = self.mouse.register_click(Instant::now(), column, row);
                match adds_cursor(event.modifiers) {
                    true => workspace.mouse_press_adding(column, row, clicks),
                    false => workspace.mouse_press(column, row, extend, clicks),
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                workspace.with_editor(|editor| editor.mouse_drag(column, row))
            }
            // Middle-clicking a tab closes it; on the text it pastes there.
            MouseEventKind::Down(MouseButton::Middle) => {
                closed = workspace.middle_press(column, row);
                paste_at = closed.is_none().then_some((column, row));
                Ok(())
            }
            MouseEventKind::Up(MouseButton::Left) => {
                workspace.with_editor(|editor| editor.mouse_release());
                Ok(())
            }
            // With `shift` the wheel scrolls sideways.
            MouseEventKind::ScrollUp if extend => {
                workspace.with_editor_at(column, row, |editor| editor.scroll_columns(-1));
                Ok(())
            }
            MouseEventKind::ScrollDown if extend => {
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

        if let Some((column, row)) = paste_at {
            self.paste_at(column, row);
        }

        if let Err(error) = result {
            self.state.notify_error(error.to_string());
        }

        self.needs_redraw = true;
    }
}

/// The stages that look at an event over the panes, first to last: each is given the event
/// and says whether it took it. The line between two panes is held until the button is let
/// go, wherever the pointer goes meanwhile; the tabs of a bar come before what is inside it,
/// as they are in its frame.
const PANE_STAGES: [fn(&mut App, MouseEvent) -> bool; 13] = [
    App::click_in_name_prompt,
    App::handle_divider_mouse,
    App::handle_definitions_mouse,
    App::handle_run_picker_mouse,
    App::handle_actions_mouse,
    App::handle_palette_mouse,
    App::handle_menu_bar_mouse,
    App::handle_bar_tab_mouse,
    App::handle_plugin_mouse,
    App::handle_errors_mouse,
    App::handle_debug_mouse,
    App::handle_terminal_mouse,
    App::handle_menu_mouse,
];

fn is_wheel(kind: MouseEventKind) -> bool {
    matches!(kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown)
}

#[cfg(test)]
mod tests;

/// Whether a left press with these modifiers adds a cursor: `alt` alone.
fn adds_cursor(modifiers: KeyModifiers) -> bool {
    modifiers.contains(KeyModifiers::ALT)
        && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::SHIFT)
}

impl App {
    /// A click on the text of the name box puts the text cursor there. Returns whether
    /// it did.
    fn click_in_name_prompt(&mut self, event: MouseEvent) -> bool {
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return false;
        }
        let Some(mut prompt) = self.state.take_name_prompt() else {
            return false;
        };
        let placed = prompt.click(event.column, event.row);
        self.state.open_name_prompt(prompt);
        placed
    }
}
