//! Messages for the user: errors, confirmations and hints.
//!
//! Info messages go away on their own after a few seconds. Errors stay until
//! the user presses a key or clicks, so a failure is not missed while looking
//! elsewhere; the key still does what it normally does. Messages are shown
//! stacked at the bottom right, the newest at the bottom.

use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::ui::theme::Theme;

/// How long an info message stays.
pub(crate) const INFO_LIFETIME: Duration = Duration::from_secs(4);
/// Messages kept; the oldest go first.
const MAX_STORED: usize = 20;
const WIDTH: u16 = 50;
/// Border plus one cell of padding on each side.
const CHROME_WIDTH: u16 = 4;
const MAX_TEXT_LINES: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    Info,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Notification {
    pub(crate) level: Level,
    pub(crate) text: Box<str>,
    /// When an info message goes away; errors have none.
    expires: Option<Instant>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Notifications {
    /// Oldest first.
    items: Vec<Notification>,
}

impl Notifications {
    /// Shows `text`. The same message shown again replaces the earlier one
    /// (and becomes the newest), so repeating an action does not pile up
    /// copies.
    pub(crate) fn push(&mut self, level: Level, text: impl Into<Box<str>>, now: Instant) {
        let text = text.into();
        self.items
            .retain(|item| !(item.level == level && item.text == text));
        let expires = match level {
            Level::Info => Some(now + INFO_LIFETIME),
            Level::Error => None,
        };
        self.items.push(Notification {
            level,
            text,
            expires,
        });
        if self.items.len() > MAX_STORED {
            self.items.remove(0);
        }
    }

    /// Removes every error. Returns whether there was one.
    pub(crate) fn dismiss_errors(&mut self) -> bool {
        let before = self.items.len();
        self.items.retain(|item| item.level != Level::Error);
        self.items.len() != before
    }

    /// Removes the info messages that are over by `now`. Returns whether any
    /// went.
    pub(crate) fn expire(&mut self, now: Instant) -> bool {
        let before = self.items.len();
        self.items
            .retain(|item| item.expires.is_none_or(|at| at > now));
        self.items.len() != before
    }

    /// When the next info message goes away, if there is one.
    pub(crate) fn next_expiry(&self) -> Option<Instant> {
        self.items.iter().filter_map(|item| item.expires).min()
    }

    #[cfg(test)]
    pub(crate) fn latest(&self) -> Option<&Notification> {
        self.items.last()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }

    #[cfg(test)]
    pub(crate) fn has_errors(&self) -> bool {
        self.items.iter().any(|item| item.level == Level::Error)
    }

    /// Draws the messages that fit between the top of the screen and `bottom`
    /// (a row), newest nearest to it.
    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme, bottom: u16) {
        let screen = frame.area();
        let width = WIDTH.min(screen.width);
        if width <= CHROME_WIDTH {
            return;
        }
        let text_width = usize::from(width - CHROME_WIDTH);
        let mut floor = bottom.min(screen.bottom());

        for item in self.items.iter().rev() {
            let mut lines = wrap(&item.text, text_width);
            lines.truncate(MAX_TEXT_LINES);
            let height = lines.len() as u16 + 2;
            if floor < screen.y + height {
                break;
            }
            floor -= height;
            let area = Rect {
                x: screen.right() - width,
                y: floor,
                width,
                height,
            };
            draw_one(frame, theme, item.level, lines, area);
        }
    }
}

fn draw_one(frame: &mut Frame, theme: &Theme, level: Level, lines: Vec<String>, area: Rect) {
    let (slot, title) = match level {
        Level::Info => ("notification.info", None),
        Level::Error => ("notification.error", Some(" Error ")),
    };
    let mut block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.style(slot));
    if let Some(title) = title {
        block = block.title(Line::styled(title, theme.style(slot)));
    }
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let padded = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    };
    let lines: Vec<Line> = lines.into_iter().map(Line::from).collect();
    frame.render_widget(Paragraph::new(lines), padded);
}

/// Breaks `text` into lines of at most `width` cells, at spaces where
/// possible and in the middle of a word when a word is longer than a line.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let mut word = word;
            while word.width() > width {
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                let (head, tail) = split_at_width(word, width);
                lines.push(head.to_owned());
                word = tail;
            }
            let needed = match line.is_empty() {
                true => word.width(),
                false => line.width() + 1 + word.width(),
            };
            if needed > width {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Splits `text` after as many characters as fit in `width` cells (at least
/// one, so the caller always makes progress).
fn split_at_width(text: &str, width: usize) -> (&str, &str) {
    let mut used = 0;
    for (index, c) in text.char_indices() {
        let cell = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if used + cell > width && index > 0 {
            return text.split_at(index);
        }
        used += cell;
    }
    (text, "")
}
