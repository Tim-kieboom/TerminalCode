//! Messages for the user: errors, confirmations and hints.
//!
//! Info messages go away on their own after a few seconds. Errors stay until
//! the user presses a key or clicks, so a failure is not missed while looking
//! elsewhere; the key still does what it normally does. Messages are shown
//! stacked at the bottom right, the newest at the bottom.

use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::components::text::wrap_words;
use crate::ui::theme::Theme;

/// How long an info message stays when the settings do not say.
pub(crate) const INFO_LIFETIME: Duration = Duration::from_secs(4);
/// Messages kept. When there are more, the oldest info messages go first, then
/// the oldest errors.
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
    /// Tells this message from the others, so a click on it after other
    /// messages came and went still finds it.
    id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Notifications {
    /// Oldest first.
    items: Vec<Notification>,
    next_id: u64,
    /// How long an info message stays, for the ones shown from now on.
    lifetime: Duration,
}

impl Default for Notifications {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            next_id: 0,
            lifetime: INFO_LIFETIME,
        }
    }
}

/// What a screen cell shown by [`Notifications::render`] belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Hit {
    /// The message with this id.
    Message(u64),
    /// The "+N more" line; the ids are the messages that were shown.
    More(Vec<u64>),
}

impl Notifications {
    /// Sets how long the info messages shown from now on stay. Errors stay
    /// until the user acts, whatever this is.
    pub(crate) fn set_lifetime(&mut self, lifetime: Duration) {
        self.lifetime = lifetime;
    }

    /// Shows `text`. The same message shown again replaces the earlier one
    /// (and becomes the newest), so repeating an action does not pile up
    /// copies.
    pub(crate) fn push(&mut self, level: Level, text: impl Into<Box<str>>, now: Instant) {
        let text = text.into();
        self.items
            .retain(|item| !(item.level == level && item.text == text));
        let expires = match level {
            Level::Info => Some(now + self.lifetime),
            Level::Error => None,
        };
        self.next_id += 1;
        self.items.push(Notification {
            level,
            text,
            expires,
            id: self.next_id,
        });
        if self.items.len() > MAX_STORED {
            self.evict_one();
        }
    }

    /// Drops one message to make room. Errors stay until the user acts, so the
    /// oldest info message goes first (which can be the one just added, if it
    /// is the only one); only when everything stored is an error does the oldest
    /// error go.
    fn evict_one(&mut self) {
        let victim = self
            .items
            .iter()
            .position(|item| item.level == Level::Info)
            .unwrap_or(0);
        self.items.remove(victim);
    }

    /// Removes the message with `id`. Returns whether it was still there.
    pub(crate) fn dismiss(&mut self, id: u64) -> bool {
        let before = self.items.len();
        self.items.retain(|item| item.id != id);
        self.items.len() != before
    }

    /// Removes the info messages that are not among `shown`, those a "+N more"
    /// line stands for. Errors are kept: they have to be seen first. Returns
    /// whether any went.
    pub(crate) fn dismiss_hidden_info(&mut self, shown: &[u64]) -> bool {
        let before = self.items.len();
        self.items
            .retain(|item| item.level != Level::Info || shown.contains(&item.id));
        self.items.len() != before
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

    /// The text of every message, oldest first.
    #[cfg(test)]
    pub(crate) fn texts(&self) -> Vec<&str> {
        self.items.iter().map(|item| &*item.text).collect()
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
    /// (a row), newest nearest to it. When not all fit, the errors are drawn
    /// first, because they stay until the user acts, and the newest info
    /// messages after them. A line "[+N more]" above the stack counts the ones
    /// that did not fit.
    ///
    /// Returns where each thing drawn is, for clicks.
    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme, bottom: u16) -> Vec<(Hit, Rect)> {
        let screen = frame.area();
        let width = WIDTH.min(screen.width);
        if width <= CHROME_WIDTH {
            return Vec::new();
        }
        let mut floor = bottom.min(screen.bottom());
        let available = floor.saturating_sub(screen.y);
        let text_width = usize::from(width - CHROME_WIDTH);

        let mut fitting = self.fitting(text_width, available);
        // The line that counts the rest takes a row of its own.
        if fitting.len() < self.items.len() && available > 0 {
            fitting = self.fitting(text_width, available - 1);
        }
        let hidden = self.items.len() - fitting.len();
        let shown: Vec<u64> = fitting
            .iter()
            .map(|(index, _)| self.items[*index].id)
            .collect();

        let mut hits = Vec::new();
        for (index, lines) in fitting.into_iter().rev() {
            let height = lines.len() as u16 + 2;
            floor -= height;
            let area = Rect {
                x: screen.right() - width,
                y: floor,
                width,
                height,
            };
            let item = &self.items[index];
            draw_one(frame, theme, item.level, lines, area);
            hits.push((Hit::Message(item.id), area));
        }

        if hidden > 0 && floor > screen.y {
            let area = Rect {
                x: screen.right() - width,
                y: floor - 1,
                width,
                height: 1,
            };
            let text = format!("[+{hidden} more]");
            crate::ui::backdrop::clear(frame, theme, area);
            frame.render_widget(
                Paragraph::new(text)
                    .alignment(Alignment::Right)
                    .style(theme.style("notification.more")),
                area,
            );
            hits.push((Hit::More(shown), area));
        }
        hits
    }

    /// The messages that fit in `available` rows, as their index and their
    /// wrapped text, oldest first. Errors are taken before info messages, and
    /// the newest before the older ones of the same level.
    fn fitting(&self, text_width: usize, available: u16) -> Vec<(usize, Vec<String>)> {
        let newest_first = |level: Level| {
            self.items
                .iter()
                .enumerate()
                .rev()
                .filter(move |(_, item)| item.level == level)
        };
        let mut fitting = Vec::new();
        let mut used = 0_u16;
        for (index, item) in newest_first(Level::Error).chain(newest_first(Level::Info)) {
            let mut lines = wrap_words(&item.text, text_width);
            lines.truncate(MAX_TEXT_LINES);
            let height = lines.len() as u16 + 2;
            if used.saturating_add(height) > available {
                break;
            }
            used += height;
            fitting.push((index, lines));
        }
        fitting.sort_by_key(|(index, _)| *index);
        fitting
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
    crate::ui::backdrop::clear(frame, theme, area);
    frame.render_widget(block, area);

    let padded = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    };
    let lines: Vec<Line> = lines.into_iter().map(Line::from).collect();
    frame.render_widget(Paragraph::new(lines), padded);
}
