//! The terminal pane: showing it, the tabs and the shells in them, and
//! hearing from the shells.

use std::path::Path;
use std::time::Instant;

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::app::App;
use crate::app::state::Focus;
use crate::components::terminal::MAX_TABS;
use crate::event::Event;
use crate::event::mouse::Clicks;
use crate::pty::{Session, Shell, SpawnConfig, encode_mouse};
use crate::ui::ComponentKind;

/// Rows and columns a shell gets before the pane has been laid out.
const FIRST_SIZE: (u16, u16) = (24, 80);
/// How many lines the wheel moves the terminal's history by one notch.
const WHEEL_LINES: isize = 3;

impl App {
    /// Hides the terminal pane when the keyboard is in it, gives the keyboard
    /// to it when it is shown but the keyboard is elsewhere, and shows it
    /// otherwise. The first time it is shown, and whenever every shell in it
    /// has ended, a shell starts in the project root.
    pub(in crate::app) fn toggle_terminal(&mut self) {
        let shown = self.state.is_showing(&ComponentKind::Terminal);
        if shown && self.state.focus() == Focus::Terminal {
            self.state.hide_terminal();
            self.state.set_focus(Focus::Editor);
            return;
        }
        self.show_terminal_with_keyboard();
    }

    /// Shows the terminal pane and gives it the keyboard, starting a shell when
    /// none is running. Does nothing when the layout has no terminal.
    pub(in crate::app) fn show_terminal_with_keyboard(&mut self) {
        if !self.show_terminal_pane() {
            return;
        }
        if !self.state.terminal().any_running() {
            self.state.terminal_mut().remove_ended();
            if !self.start_tab() {
                self.hide_pane_without_tabs();
                return;
            }
        }
        self.state.set_focus(Focus::Terminal);
    }

    /// At startup: a pane that starts shown (the layout says so, or it was shown
    /// last time) gets its shell, and the keyboard stays where it is.
    pub(crate) fn start_terminal_if_shown(&mut self) {
        if self.state.is_visible(&ComponentKind::Terminal)
            && self.state.layout().contains(&ComponentKind::Terminal)
            && !self.start_tab()
        {
            self.hide_pane_without_tabs();
        }
    }

    /// Opens a new tab with a shell in it, showing the pane if it is hidden.
    pub(in crate::app) fn new_terminal_tab(&mut self) {
        if !self.show_terminal_pane() {
            return;
        }
        if self.state.terminal().tab_count() >= MAX_TABS {
            self.state
                .notify_error(format!("at most {MAX_TABS} terminal tabs"));
            return;
        }
        if !self.start_tab() {
            self.hide_pane_without_tabs();
            return;
        }
        self.state.set_focus(Focus::Terminal);
    }

    /// Moves to another tab: the next one for a positive `step`, the previous
    /// one for a negative.
    pub(in crate::app) fn step_terminal_tab(&mut self, step: isize) {
        if !self.show_terminal_pane() {
            return;
        }
        if step > 0 {
            self.state.terminal_mut().select_next();
        } else {
            self.state.terminal_mut().select_previous();
        }
        self.state.set_focus(Focus::Terminal);
    }

    /// Goes to tab `number`, counted from 1.
    pub(in crate::app) fn select_terminal_tab(&mut self, number: usize) {
        if number == 0 || number > self.state.terminal().tab_count() {
            return;
        }
        self.state.terminal_mut().select(number - 1);
        self.needs_redraw = true;
    }

    /// Closes the active tab, which stops its shell. Closing the last one
    /// hides the pane.
    pub(in crate::app) fn close_terminal_tab(&mut self) {
        let active = self.state.terminal().active_tab();
        self.close_tab_number(active);
    }

    fn close_tab_number(&mut self, index: usize) {
        self.state.terminal_mut().close(index);
        if self.state.terminal().tab_count() == 0 {
            self.state.hide_terminal();
            if self.state.focus() == Focus::Terminal {
                self.state.set_focus(Focus::Editor);
            }
        }
        self.needs_redraw = true;
    }

    /// Makes the pane visible. Returns whether the layout has one.
    pub(in crate::app) fn show_terminal_pane(&mut self) -> bool {
        if !self.state.layout().contains(&ComponentKind::Terminal) {
            self.state.notify("the layout has no terminal");
            return false;
        }
        self.state.show_terminal();
        true
    }

    /// Gives the keyboard to the explorer, showing it if it was hidden.
    pub(in crate::app) fn focus_explorer_shown(&mut self) {
        self.state.show_explorer();
        self.state.set_focus(Focus::Explorer);
    }

    /// Starts a shell in a new tab, in the project root. Returns whether one
    /// started. When the shell cannot start, the platform's own is tried once
    /// and, if that works, it is the shell for the rest of the run; the error
    /// that says so stays on screen until it is dismissed. Failing both says why.
    fn start_tab(&mut self) -> bool {
        let Some(events) = self.background.sender() else {
            self.state
                .notify_error("the terminal needs the event loop, which is not running");
            return false;
        };
        let (rows, columns) = self.terminal_dimensions();
        let directory = self.project_root();
        let spawn = |shell: &Shell| {
            let config = SpawnConfig {
                shell: shell.clone(),
                directory: directory.clone(),
                env: Vec::new(),
                rows,
                columns,
            };
            let events = events.clone();
            Session::spawn(&config, move || {
                // Delivery matters more than speed here: a lost wake-up would
                // leave the screen stale until the next key.
                let _ = events.blocking_send(Event::TerminalChanged);
            })
        };

        let (shell, session, notice) = match spawn(&self.terminal_shell) {
            Ok(session) => (self.terminal_shell.clone(), session, None),
            Err(error) => {
                let fallback = self.fallback_shell.clone();
                match (fallback != self.terminal_shell).then(|| spawn(&fallback)) {
                    Some(Ok(session)) => {
                        let notice = format!("{error}; started `{}` instead", fallback.program);
                        (fallback, session, Some(notice))
                    }
                    _ => {
                        self.state.notify_error(error.to_string());
                        return false;
                    }
                }
            }
        };
        let title = program_name(&shell.program);
        self.terminal_shell = shell;
        self.state.terminal_mut().start(session, title);
        if let Some(notice) = notice {
            self.state.notify_error(notice);
        }
        true
    }

    /// The rows and columns a new session gets: the text area of the pane, less the row of the
    /// tab bar.
    pub(in crate::app) fn terminal_dimensions(&self) -> (u16, u16) {
        let (rows, columns) = self
            .state
            .terminal_size_now()
            .or(self.state.terminal().size())
            .unwrap_or(FIRST_SIZE);
        (rows.saturating_sub(1).max(1), columns)
    }

    /// After a shell could not start: a pane with no tab is no use, so it is
    /// hidden again and the keyboard stays where it was.
    fn hide_pane_without_tabs(&mut self) {
        if self.state.terminal().tab_count() == 0 {
            self.state.hide_terminal();
            if self.state.focus() == Focus::Terminal {
                self.state.set_focus(Focus::Editor);
            }
        }
        self.needs_redraw = true;
    }

    /// A shell printed something or ended: draw again, and say when one ended.
    pub(in crate::app) fn terminal_changed(&mut self) {
        self.needs_redraw = true;
        let many = self.state.terminal().tab_count() > 1;
        for exit in self.state.terminal_mut().take_exits() {
            if exit.run.is_some() {
                self.run_tab_ended(&exit);
                continue;
            }
            let number = exit.number;
            let which = if many {
                format!("terminal {number}'s shell")
            } else {
                "the terminal's shell".to_owned()
            };
            let how = match exit.code {
                0 => "ended".to_owned(),
                code => format!("ended with code {code}"),
            };
            self.state.notify(format!(
                "{which} {how} (toggle the pane to start another, ctrl+b w closes its tab)"
            ));
        }
    }

    /// Mouse input over the terminal pane. Returns whether it was used up.
    ///
    /// A click on a tab picks it (middle click closes it). In the text, a
    /// program that asked for the mouse gets the events (unless shift is held,
    /// the usual way to get at the terminal's own behavior); otherwise a click
    /// gives the pane the keyboard and the wheel moves through the history.
    pub(in crate::app) fn handle_terminal_mouse(&mut self, event: MouseEvent) -> bool {
        if self.modal_open()
            || !self.state.is_showing(&ComponentKind::Terminal)
            || !self.state.terminal().contains(event.column, event.row)
        {
            // The pointer left: nothing there is underlined any more.
            if self.state.terminal_mut().set_link(None) {
                self.needs_redraw = true;
            }
            return false;
        }
        if self.state.terminal().is_add_button(event.column, event.row) {
            if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
                self.new_terminal_tab();
            }
            self.needs_redraw = true;
            return true;
        }
        if let Some(index) = self.state.terminal().tab_at(event.column, event.row) {
            match event.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    self.state.terminal_mut().select(index);
                    self.state.set_focus(Focus::Terminal);
                }
                MouseEventKind::Down(MouseButton::Middle) => self.close_tab_number(index),
                _ => {}
            }
            self.needs_redraw = true;
            return true;
        }
        if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
            self.state.set_focus(Focus::Terminal);
            self.needs_redraw = true;
        }
        if self.terminal_link_mouse(event) {
            return true;
        }
        if self.report_mouse_to_shell(event) {
            return true;
        }
        // A program that has not asked for the mouse (or `shift` held, which is how a program
        // that has is told it does not) leaves the left button to selecting text.
        if let MouseEventKind::Down(MouseButton::Left) = event.kind {
            self.begin_terminal_selection(event);
            return true;
        }
        match event.kind {
            MouseEventKind::ScrollUp => self.scroll_terminal(WHEEL_LINES),
            MouseEventKind::ScrollDown => self.scroll_terminal(-WHEEL_LINES),
            // Everything else over the pane stays out of the editor.
            _ => {}
        }
        true
    }

    /// A press on the text: drops the selection there was and starts a new one, a word for a
    /// double click and a line for a triple click (those are copied at once).
    fn begin_terminal_selection(&mut self, event: MouseEvent) {
        let Some(cell) = self.state.terminal().screen_cell(event.column, event.row) else {
            return;
        };
        let (row, column) = (cell.1, cell.0);
        let clicks = self
            .mouse
            .register_click(Instant::now(), event.column, event.row);
        let terminal = self.state.terminal_mut();
        terminal.clear_selection();
        match clicks {
            Clicks::Single => {
                terminal.begin_selection((row, column));
                self.dragging = Some(super::dividers::Dragging::Terminal);
            }
            Clicks::Double => terminal.select_word((row, column)),
            Clicks::Triple => terminal.select_line((row, column)),
        }
        if clicks != Clicks::Single {
            self.copy_terminal_selection(false);
        }
        self.needs_redraw = true;
    }

    /// The pointer moved with the button down: the selection goes as far as the cell under it,
    /// or the edge it left by.
    pub(super) fn extend_terminal_selection(&mut self, column: u16, row: u16) {
        if let Some(cell) = self.state.terminal().clamped_cell(column, row) {
            self.state.terminal_mut().extend_selection(cell);
            self.needs_redraw = true;
        }
    }

    /// Puts the selected text on the clipboard. `say`: tell the user, which is for when they
    /// asked (a copy on letting go is quiet).
    pub(super) fn copy_terminal_selection(&mut self, say: bool) {
        let Some(text) = self.state.terminal().selected_text().map(str::to_owned) else {
            if say {
                self.state.notify("nothing is selected in the terminal");
            }
            return;
        };
        let characters = text.chars().count();
        self.set_register(crate::clipboard::Register::charwise(text));
        if say {
            self.state.notify(format!("copied {characters} characters"));
        }
    }

    /// Sends the event to the shell as the mouse report it asked for. Returns
    /// whether it did.
    fn report_mouse_to_shell(&mut self, event: MouseEvent) -> bool {
        if event.modifiers.contains(KeyModifiers::SHIFT) {
            return false;
        }
        let Some((column, row)) = self.state.terminal().screen_cell(event.column, event.row) else {
            return false;
        };
        let Some(session) = self.state.terminal().session() else {
            return false;
        };
        let (mode, encoding) = session.with_screen(|screen| {
            (
                screen.mouse_protocol_mode(),
                screen.mouse_protocol_encoding(),
            )
        });
        let Some(bytes) = encode_mouse(event.kind, event.modifiers, column, row, mode, encoding)
        else {
            return false;
        };
        self.send_to_shell(&bytes);
        true
    }
}

/// The name of a program for a tab: the file name, without the folders.
fn program_name(program: &str) -> String {
    Path::new(program).file_name().map_or_else(
        || program.to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}
