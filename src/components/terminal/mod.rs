//! The integrated terminal pane: a shell's screen drawn inside a frame, and
//! the size the shell is told to have.

use ratatui::Frame;

use crate::app::state::AppState;
use crate::pty::Session;
use crate::ui::Render;
use crate::ui::layout::Placement;

mod render;
#[cfg(test)]
mod tests;

/// The pane and the shell running in it, once there is one.
#[derive(Debug, Default)]
pub(crate) struct TerminalPane {
    session: Option<Session>,
    /// Rows and columns of the text area, as of the last layout pass. Kept
    /// while the pane is hidden, so the shell keeps the size it last had.
    size: Option<(u16, u16)>,
    /// The end of the shell has been reported to the user.
    exit_reported: bool,
}

impl TerminalPane {
    /// Gives the pane a running shell, replacing the one it had. The shell is
    /// told the pane's size at once if it is known.
    pub(crate) fn start(&mut self, session: Session) {
        if let Some((rows, columns)) = self.size {
            session.resize(rows, columns);
        }
        self.session = Some(session);
        self.exit_reported = false;
    }

    #[cfg(test)]
    pub(crate) fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// Whether there is a shell that has not ended.
    pub(crate) fn is_running(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.exit_code().is_none() && !session.output_ended())
    }

    /// The size the shell has, or will have once the pane is drawn.
    pub(crate) fn size(&self) -> Option<(u16, u16)> {
        self.size
    }

    /// The exit code of a shell that has ended, once; asking again gives
    /// `None` until a new shell ends.
    pub(crate) fn take_exit(&mut self) -> Option<u32> {
        if self.exit_reported {
            return None;
        }
        let code = self.session.as_ref()?.finished()?;
        self.exit_reported = true;
        Some(code)
    }
}

impl Render for TerminalPane {
    fn prepare(&mut self, placement: &Placement) {
        let inner = placement.frame.inner(placement.area);
        let size = (inner.height, inner.width);
        let Some(session) = &self.session else {
            self.size = Some(size);
            return;
        };
        // The screen is about to be drawn: whatever it shows now is seen, so
        // the next change wakes the app again.
        session.take_dirty();
        if self.size != Some(size) && size.0 > 0 && size.1 > 0 {
            session.resize(size.0, size.1);
        }
        self.size = Some(size);
    }

    fn render(&self, frame: &mut Frame, state: &AppState, placement: &Placement) {
        render::draw(frame, state, placement, self.session.as_ref());
    }
}
