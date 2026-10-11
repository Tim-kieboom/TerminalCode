//! Keys while the terminal pane has the keyboard: everything goes to the
//! shell except `ctrl+b`, which is the chord the editor keeps, and the chord
//! that toggles the pane (`ctrl+`` by default), which hides it again.
//!
//! After `ctrl+b`: `ctrl+b` again sends a literal `ctrl+b`; `e`, `x` and `t`
//! move the keyboard (editor, explorer) or hide the pane; `esc` cancels;
//! anything else is looked up in the normal keymap, so a binding such as
//! `ctrl+p` works behind the prefix. A key that is bound to nothing cancels it
//! and says so; waiting too long cancels it silently.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::app::keys::EDITOR_CONTEXTS;
use crate::app::state::Focus;
use crate::event::action::Action;
use crate::keymap::{KeyChord, Outcome};
use crate::pty::{encode_key, encode_paste};

/// How long `ctrl+b` waits for the key after it.
const PREFIX_WAIT: Duration = Duration::from_millis(1000);

fn prefix() -> KeyChord {
    KeyChord::new(KeyCode::Char('b'), KeyModifiers::CONTROL)
}

impl App {
    pub(super) fn handle_terminal_key(&mut self, key: KeyEvent) {
        let Some(chord) = KeyChord::from_event(key) else {
            return;
        };

        if self.terminal_prefix.take().is_some() {
            self.state.set_terminal_prefix(false);
            self.needs_redraw = true;
            self.key_after_prefix(chord);
            return;
        }

        if chord == prefix() {
            self.terminal_prefix = Some(Instant::now() + PREFIX_WAIT);
            self.state.set_terminal_prefix(true);
            self.needs_redraw = true;
            return;
        }

        // The chord that shows the pane also closes it: pressing it again hides
        // the pane and puts the keyboard back in the editor. What it is comes
        // from the keymap, so a rebound key works the same.
        if self.keyboard.action_for(&EDITOR_CONTEXTS, chord) == Some(&Action::ToggleTerminal) {
            self.toggle_terminal();
            self.needs_redraw = true;
            return;
        }

        let Some(session) = self.state.terminal().session() else {
            return;
        };

        let (application_cursor, alternate) =
            session.with_screen(|screen| (screen.application_cursor(), screen.alternate_screen()));

        // The history belongs to the normal screen; a full-screen program has
        // none, and gets these keys itself.
        if !alternate && self.scroll_with_key(key) {
            return;
        }

        if let Some(bytes) = encode_key(key, application_cursor) {
            self.send_to_shell(&bytes);
        }
    }

    /// Pasted text goes to the shell, as a bracketed paste if the program
    /// asked for those.
    pub(super) fn paste_into_shell(&mut self, text: &str) {
        let Some(session) = self.state.terminal().session() else {
            return;
        };
        let bracketed = session.with_screen(|screen| screen.bracketed_paste());
        self.send_to_shell(&encode_paste(text, bracketed));
        self.needs_redraw = true;
    }

    fn send_to_shell(&mut self, bytes: &[u8]) {
        let Some(session) = self.state.terminal().session() else {
            return;
        };

        // Typing, like pasting, brings the view back to what the shell is doing.
        if session.scrollback_offset() > 0 {
            session.scroll_to_bottom();
            self.needs_redraw = true;
        }

        if !self.state.terminal().is_running() {
            self.state.notify(
                "the terminal's shell has ended (ctrl+b t hides the pane; showing it again starts a new one)",
            );
            return;
        }

        let written = session.write(bytes);
        if !written.complete {
            self.state.notify_error(format!(
                "the terminal is not reading: {} of {} bytes were sent",
                written.sent,
                bytes.len()
            ));
        }
    }

    fn key_after_prefix(&mut self, chord: KeyChord) {
        if chord == prefix() {
            self.send_to_shell(&[0x02]);
            return;
        }

        if chord == KeyChord::new(KeyCode::Esc, KeyModifiers::NONE) {
            return;
        }

        match chord.typed_char() {
            Some('e') => self.state.set_focus(Focus::Editor),
            Some('x') => self.focus_explorer_shown(),
            Some('t') => self.toggle_terminal(),
            _ => self.run_through_keymap(chord),
        }
    }

    /// Looks the key up as the editor would; behind the prefix a binding that
    /// has more keys to come keeps going through the keymap.
    fn run_through_keymap(&mut self, chord: KeyChord) {
        let resolution = self.keyboard.feed(&EDITOR_CONTEXTS, chord);
        match resolution.outcome {
            Outcome::Action(action) => self.run_action(action),
            Outcome::Pending => {}
            Outcome::Unbound(chord) => self.state.notify(format!("ctrl+b {chord} is not bound")),
        }
    }

    /// Gives up on `ctrl+b` if nothing followed it in time. Returns whether
    /// that was what was pending.
    pub(super) fn expire_terminal_prefix(&mut self) -> bool {
        let Some(deadline) = self.terminal_prefix else {
            return false;
        };

        if Instant::now() >= deadline {
            self.terminal_prefix = None;
            self.state.set_terminal_prefix(false);
            self.needs_redraw = true;
        }
        true
    }

    /// When `ctrl+b` gives up waiting, if it is waiting.
    pub(super) fn terminal_prefix_deadline(&self) -> Option<Instant> {
        self.terminal_prefix
    }
}

impl App {
    /// `shift+pageup`, `shift+pagedown`, `shift+home` and `shift+end` move
    /// through the history, as in other terminals. Returns whether `key` was one.
    fn scroll_with_key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers != KeyModifiers::SHIFT {
            return false;
        }

        let Some(session) = self.state.terminal().session() else {
            return false;
        };

        let page = session.with_screen(|screen| screen.size().0.saturating_sub(1).max(1));
        match key.code {
            KeyCode::PageUp => session.scroll_by(page as isize),
            KeyCode::PageDown => session.scroll_by(-(page as isize)),
            KeyCode::Home => session.scroll_to_top(),
            KeyCode::End => session.scroll_to_bottom(),
            _ => return false,
        }
        self.needs_redraw = true;
        true
    }

    /// The wheel over the pane: into the history and back. A full-screen
    /// program has no history, so there it does nothing.
    pub(super) fn scroll_terminal(&mut self, rows: isize) {
        let Some(session) = self.state.terminal().session() else {
            return;
        };

        if session.with_screen(|screen| screen.alternate_screen()) {
            return;
        }

        session.scroll_by(rows);
        self.needs_redraw = true;
    }
}
