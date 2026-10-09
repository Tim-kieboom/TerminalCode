//! The terminal pane: showing it, starting the shell the first time, and
//! hearing from the shell.

use crate::app::App;
use crate::event::Event;
use crate::pty::{Session, SpawnConfig};

/// Rows and columns a shell gets before the pane has been laid out.
const FIRST_SIZE: (u16, u16) = (24, 80);

impl App {
    /// Hides the terminal pane, or shows it. The first time it is shown, and
    /// whenever the shell has ended, a shell starts in the project root.
    pub(in crate::app) fn toggle_terminal(&mut self) {
        if self
            .state
            .is_visible(&crate::components::ComponentKind::Terminal)
        {
            self.state.hide_terminal();
            return;
        }
        if !self
            .state
            .layout()
            .contains(&crate::components::ComponentKind::Terminal)
        {
            self.state.notify("the layout has no terminal");
            return;
        }
        self.state.show_terminal();
        if !self.state.terminal().is_running() {
            self.start_shell();
        }
    }

    fn start_shell(&mut self) {
        let Some(events) = self.background.sender() else {
            self.state
                .notify_error("the terminal needs the event loop, which is not running");
            return;
        };
        let (rows, columns) = self
            .state
            .terminal_size_now()
            .or(self.state.terminal().size())
            .unwrap_or(FIRST_SIZE);
        let config = SpawnConfig {
            shell: self.terminal_shell.clone(),
            directory: self.project_root(),
            rows,
            columns,
        };
        let session = Session::spawn(&config, move || {
            // Delivery matters more than speed here: a lost wake-up would
            // leave the screen stale until the next key.
            let _ = events.blocking_send(Event::TerminalChanged);
        });
        match session {
            Ok(session) => self.state.terminal_mut().start(session),
            Err(error) => self.state.notify_error(error.to_string()),
        }
    }

    /// The shell printed something or ended: draw again, and say when it ended.
    pub(in crate::app) fn terminal_changed(&mut self) {
        self.needs_redraw = true;
        if let Some(code) = self.state.terminal_mut().take_exit() {
            self.state.notify(match code {
                0 => "the terminal's shell ended (toggle the pane to start another)".to_owned(),
                code => format!(
                    "the terminal's shell ended with code {code} (toggle the pane to start another)"
                ),
            });
        }
    }
}
