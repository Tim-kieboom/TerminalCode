//! The toolbar of the debugger: a small box in the middle of the top of the editor while a
//! session goes, saying where the program is and with a button for each control (continue, step
//! over, into and out, restart, and stop, which closes the debugger).

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use super::model::Status;
use crate::event::action::Action;
use crate::ui::theme::Theme;

/// The controls, left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Button {
    Continue,
    Over,
    Into,
    Out,
    Restart,
    Stop,
}

impl Button {
    const ALL: [Self; 6] = [
        Self::Continue,
        Self::Over,
        Self::Into,
        Self::Out,
        Self::Restart,
        Self::Stop,
    ];

    fn glyph(self) -> &'static str {
        match self {
            Self::Continue => "▶",
            Self::Over => "↷",
            Self::Into => "↓",
            Self::Out => "↑",
            Self::Restart => "↻",
            Self::Stop => "■",
        }
    }

    pub(crate) fn action(self) -> Action {
        match self {
            Self::Continue => Action::DebugContinue,
            Self::Over => Action::DebugStepOver,
            Self::Into => Action::DebugStepInto,
            Self::Out => Action::DebugStepOut,
            Self::Restart => Action::DebugRestart,
            Self::Stop => Action::DebugStop,
        }
    }

    /// Whether the button can only be used while the program is stopped.
    fn needs_a_stop(self) -> bool {
        matches!(self, Self::Continue | Self::Over | Self::Into | Self::Out)
    }
}

/// What the top of the editor says about the session, when there is one.
fn label(status: &Status) -> Option<&'static str> {
    match status {
        Status::Starting => Some("starting"),
        Status::Running => Some("running"),
        Status::Stopped { .. } => Some("paused"),
        Status::Idle | Status::Ended(_) => None,
    }
}

/// Where the toolbar is and where its buttons are, as of the last layout pass.
#[derive(Debug, Default)]
pub(crate) struct Toolbar {
    area: Rect,
    buttons: Vec<(Rect, Button)>,
    label: Rect,
}

impl Toolbar {
    /// Puts the toolbar in the middle of the first line of text of `pane` (below its tab bar),
    /// or nowhere when there is no session or no room.
    pub(crate) fn place(&mut self, pane: Rect, status: &Status) {
        self.area = Rect::default();
        self.buttons.clear();
        let Some(label) = label(status) else {
            return;
        };
        // " paused " then " X " for each button.
        let label_width = u16::try_from(label.width() + 2).unwrap_or(u16::MAX);
        let width = label_width + 3 * 6 + 1;
        if pane.width < width + 2 || pane.height < 4 {
            return;
        }
        let x = pane.x + (pane.width - width) / 2;
        let y = pane.y + 1;
        self.area = Rect {
            x,
            y,
            width,
            height: 1,
        };
        self.label = Rect {
            x,
            y,
            width: label_width,
            height: 1,
        };
        // A column of separation between the label and the buttons.
        let mut at = x + label_width + 1;
        for button in Button::ALL {
            self.buttons.push((
                Rect {
                    x: at,
                    y,
                    width: 3,
                    height: 1,
                },
                button,
            ));
            at += 3;
        }
    }

    pub(crate) fn contains(&self, column: u16, row: u16) -> bool {
        self.area.contains(Position::new(column, row))
    }

    /// The button under a screen cell.
    pub(crate) fn button_at(&self, column: u16, row: u16) -> Option<Button> {
        let position = Position::new(column, row);
        self.buttons
            .iter()
            .find(|(area, _)| area.contains(position))
            .map(|(_, button)| *button)
    }

    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme, status: &Status) {
        let Some(text) = label(status).filter(|_| self.area.width > 0) else {
            return;
        };
        let base = theme.style("debug.toolbar");
        let stopped = matches!(status, Status::Stopped { .. });
        let mut spans = vec![
            Span::styled(format!(" {text} "), base.add_modifier(Modifier::BOLD)),
            Span::styled(" ", base),
        ];
        for button in Button::ALL {
            let style = match (button, stopped || !button.needs_a_stop()) {
                (Button::Stop, _) => theme.style("debug.toolbar.stop"),
                (_, true) => base,
                (_, false) => base.add_modifier(Modifier::DIM),
            };
            spans.push(Span::styled(format!(" {} ", button.glyph()), style));
        }
        spans.push(Span::styled(" ", base));
        frame.render_widget(Paragraph::new(Line::from(spans)).style(base), self.area);
    }

    #[cfg(test)]
    pub(crate) fn label_area(&self) -> Rect {
        self.label
    }
}
