//! A yes/no question about something that cannot be undone. Only `y` says yes;
//! Enter, Esc and anything else cancel, so a stray key never destroys anything.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

use crate::ui::theme::Theme;

const HELP: &str = "y: yes | esc, enter: cancel";
const MIN_WIDTH: u16 = 46;
const MAX_WIDTH: u16 = 80;

/// What saying yes does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Purpose {
    /// Delete `path` although open tabs of it have unsaved changes.
    DiscardAndDelete(PathBuf),
    /// Delete `path` for good, because it could not be moved to the trash.
    DeletePermanently(PathBuf),
}

/// What a key press answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Answer {
    Yes,
    Cancel,
}

impl Answer {
    /// The answer a key gives; `None` for key releases and modified keys.
    pub(crate) fn from_key(key: KeyEvent) -> Option<Self> {
        if key.kind == KeyEventKind::Release {
            return None;
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return None;
        }
        Some(match key.code {
            KeyCode::Char('y' | 'Y') => Self::Yes,
            _ => Self::Cancel,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Confirm {
    title: String,
    lines: Vec<String>,
    purpose: Purpose,
}

impl Confirm {
    /// Asks whether to delete `path` and lose the unsaved changes in `names`.
    pub(crate) fn discard_and_delete(path: &Path, names: &[String]) -> Self {
        let mut lines = vec![format!("Delete {}?", display(path)), String::new()];
        lines.push("These files have unsaved changes that will be lost:".to_owned());
        lines.extend(names.iter().map(|name| format!("  {name}")));
        Self {
            title: " Unsaved changes ".to_owned(),
            lines,
            purpose: Purpose::DiscardAndDelete(path.to_path_buf()),
        }
    }

    /// Asks whether to delete `path` for good; moving it to the trash failed
    /// because of `reason`.
    pub(crate) fn delete_permanently(path: &Path, reason: &str) -> Self {
        Self {
            title: " Cannot move to trash ".to_owned(),
            lines: vec![
                format!(
                    "{} could not be moved to the trash: {reason}",
                    display(path)
                ),
                String::new(),
                "Delete it permanently? This cannot be undone.".to_owned(),
            ],
            purpose: Purpose::DeletePermanently(path.to_path_buf()),
        }
    }

    pub(crate) fn purpose(&self) -> &Purpose {
        &self.purpose
    }

    #[cfg(test)]
    pub(crate) fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        let screen = frame.area();
        let width = MAX_WIDTH.min(screen.width).max(MIN_WIDTH.min(screen.width));
        let inner_width = usize::from(width.saturating_sub(4)).max(1);
        let wrapped: usize = self
            .lines
            .iter()
            .map(|line| line.chars().count().div_ceil(inner_width).max(1))
            .sum();
        let height = (wrapped as u16 + 4).min(screen.height);
        let area = Rect {
            x: screen.x + (screen.width - width) / 2,
            y: screen.y + (screen.height - height) / 2,
            width,
            height,
        };
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border.focused"))
            .title(Line::styled(&self.title, theme.style("pane.title")));
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);

        let body = Rect {
            height: inner.height.saturating_sub(1),
            ..inner
        };
        let help = Rect {
            y: inner.y + body.height,
            height: inner.height - body.height,
            ..inner
        };
        let lines: Vec<Line> = self.lines.iter().map(|l| Line::from(l.as_str())).collect();
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), body);
        frame.render_widget(Paragraph::new(HELP), help);
    }
}

/// The name of `path` for the question; the whole path when it has no name.
fn display(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}
