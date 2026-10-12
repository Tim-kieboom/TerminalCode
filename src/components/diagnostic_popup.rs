//! The whole text of the diagnostics on a line, in a box next to the cursor:
//! for reading a message that does not fit after the code, or that has more
//! lines than the one shown there (a compiler's notes and help come after it).
//! Opened on demand, closed by any key; the arrow and page keys scroll a
//! message that is longer than the box.

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthChar;

use crate::components::editor::render::slot_of;
use crate::lsp::{Diagnostic, Severity};
use crate::ui::theme::Theme;

/// The widest the box gets, however wide the screen.
const MAX_WIDTH: u16 = 100;
/// The narrowest it gets when the screen allows.
const MIN_WIDTH: u16 = 40;
/// Columns of the box that are not text: the border and a space inside each side.
const FRAME_COLUMNS: u16 = 4;
/// Rows that are not text: the border, and the line of help.
const FRAME_ROWS: u16 = 3;

/// What a key does to the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PopupKey {
    /// Moves the text by this many lines.
    Lines(isize),
    /// Moves the text by this many pages.
    Pages(isize),
    Close,
    /// A key release, which is nothing.
    Ignore,
}

impl PopupKey {
    pub(crate) fn from_key(key: KeyEvent) -> Self {
        if key.kind == KeyEventKind::Release {
            return Self::Ignore;
        }
        match key.code {
            KeyCode::Up => Self::Lines(-1),
            KeyCode::Down => Self::Lines(1),
            KeyCode::PageUp => Self::Pages(-1),
            KeyCode::PageDown => Self::Pages(1),
            _ => Self::Close,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    severity: Severity,
    header: String,
    message: String,
}

#[derive(Debug)]
pub(crate) struct DiagnosticPopup {
    entries: Vec<Entry>,
    /// Where the cursor is on the screen: the box goes under it, or above it
    /// when there is no room below.
    anchor: Position,
    scroll: Cell<usize>,
    /// What the last draw found, so that scrolling stops at the ends and a page
    /// is as many lines as the box shows.
    max_scroll: Cell<usize>,
    page: Cell<usize>,
}

impl DiagnosticPopup {
    /// The box for `diagnostics`, worst first.
    pub(crate) fn new(diagnostics: &[&Diagnostic], anchor: Position) -> Self {
        let mut ordered: Vec<&&Diagnostic> = diagnostics.iter().collect();
        ordered.sort_by_key(|diagnostic| (diagnostic.severity, diagnostic.range.start));
        let entries = ordered
            .into_iter()
            .map(|diagnostic| Entry {
                severity: diagnostic.severity,
                header: header_of(diagnostic),
                message: diagnostic.message.trim_end().to_owned(),
            })
            .collect();
        Self {
            entries,
            anchor,
            scroll: Cell::new(0),
            max_scroll: Cell::new(0),
            page: Cell::new(1),
        }
    }

    /// Moves the text by `lines`, stopping at the ends.
    pub(crate) fn scroll_lines(&self, lines: isize) {
        let moved = (self.scroll.get() as isize + lines).clamp(0, self.max_scroll.get() as isize);
        self.scroll.set(moved as usize);
    }

    /// Moves the text by `pages`, a page being as many lines as the box shows.
    pub(crate) fn scroll_pages(&self, pages: isize) {
        self.scroll_lines(pages * self.page.get() as isize);
    }

    #[cfg(test)]
    pub(crate) fn scroll_position(&self) -> usize {
        self.scroll.get()
    }

    /// The width of the longest line of text, before any wrapping.
    fn longest_line(&self) -> usize {
        use unicode_width::UnicodeWidthStr;
        self.entries
            .iter()
            .flat_map(|entry| entry.message.lines().chain([entry.header.as_str()]))
            .map(UnicodeWidthStr::width)
            .max()
            .unwrap_or(0)
    }

    /// The text as it is shown, line by line, for a box with `width` columns of text.
    pub(crate) fn lines(&self, width: usize) -> Vec<(Option<Severity>, String)> {
        let mut lines = Vec::new();
        for (index, entry) in self.entries.iter().enumerate() {
            if index > 0 {
                lines.push((None, String::new()));
            }
            lines.push((Some(entry.severity), entry.header.clone()));
            for text in wrap_text(&entry.message, width) {
                lines.push((None, text));
            }
        }
        lines
    }

    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        let screen = frame.area();
        // As wide as the longest line, within what the screen allows.
        let widest = u16::try_from(self.longest_line()).unwrap_or(u16::MAX);
        let width = widest
            .saturating_add(FRAME_COLUMNS)
            .clamp(MIN_WIDTH, MAX_WIDTH)
            .min(screen.width.saturating_sub(2))
            .max(MIN_WIDTH.min(screen.width));
        let text_width = usize::from(width.saturating_sub(FRAME_COLUMNS)).max(1);
        let lines = self.lines(text_width);

        let most = screen.height.saturating_sub(2).max(FRAME_ROWS + 1);
        let height = (lines.len() as u16 + FRAME_ROWS)
            .min(most)
            .min(screen.height);
        let rows = usize::from(height.saturating_sub(FRAME_ROWS));
        self.max_scroll.set(lines.len().saturating_sub(rows));
        self.page.set(rows.max(1));
        let scroll = self.scroll.get().min(self.max_scroll.get());
        self.scroll.set(scroll);

        let area = place(screen, self.anchor, width, height);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border.focused"))
            .title(Line::styled(" Diagnostics ", theme.style("pane.title")));
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);

        let body = Rect {
            x: inner.x + 1,
            width: inner.width.saturating_sub(2),
            height: inner.height.saturating_sub(1),
            ..inner
        };
        let shown: Vec<Line> = lines
            .iter()
            .skip(scroll)
            .take(usize::from(body.height))
            .map(|(severity, text)| match severity {
                Some(severity) => Line::from(Span::styled(
                    text.clone(),
                    theme.style(slot_of(*severity)).add_modifier(Modifier::BOLD),
                )),
                None => Line::from(text.clone()),
            })
            .collect();
        frame.render_widget(Paragraph::new(shown), body);

        let help = match self.max_scroll.get() {
            0 => "any key: close".to_owned(),
            more => format!(
                "↑↓/pgup/pgdn: scroll {}/{} | esc: close",
                scroll + 1,
                more + 1
            ),
        };
        let help_area = Rect {
            x: body.x,
            y: inner.y + body.height,
            width: body.width,
            height: inner.height - body.height,
        };
        frame.render_widget(Paragraph::new(help), help_area);
    }
}

/// `error[E0308] (rustc)`: what kind it is, and the code and the tool when it has them.
fn header_of(diagnostic: &Diagnostic) -> String {
    let word = match diagnostic.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Information => "info",
        Severity::Hint => "hint",
    };
    let code = diagnostic
        .code
        .as_deref()
        .map(|code| format!("[{code}]"))
        .unwrap_or_default();
    let source = diagnostic
        .source
        .as_deref()
        .map(|source| format!(" ({source})"))
        .unwrap_or_default();
    format!("{word}{code}{source}")
}

/// Where a box of `width` by `height` goes on `screen` for a cursor at `anchor`:
/// under the cursor's line, above it when it would run off the bottom, and
/// always inside the screen.
fn place(screen: Rect, anchor: Position, width: u16, height: u16) -> Rect {
    let x = anchor
        .x
        .min(screen.right().saturating_sub(width))
        .max(screen.x);
    let below = anchor.y.saturating_add(1);
    let y = if below + height <= screen.bottom() {
        below
    } else if anchor.y >= screen.y + height {
        anchor.y - height
    } else {
        screen.bottom().saturating_sub(height).max(screen.y)
    };
    Rect {
        x,
        y,
        width: width.min(screen.width),
        height: height.min(screen.height),
    }
}

/// `text` broken into lines of at most `width` columns: at a space where there
/// is one, and in the middle of a word that is longer than a line. Line breaks
/// in `text` stay, and so do the spaces at the start of a line (a compiler
/// indents what it quotes).
pub(crate) fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for source in text.split('\n') {
        let source = source.trim_end_matches('\r').trim_end();
        let mut current = String::new();
        let mut used = 0;
        // Where the line can be broken: after the last space, and how wide the text before it is.
        let mut space: Option<(usize, usize)> = None;
        for character in source.chars() {
            let columns = character.width().unwrap_or(0);
            // A space may hang over the edge: it is trimmed where the line breaks.
            if used + columns > width && used > 0 && character != ' ' {
                match space.take() {
                    Some((at, before)) => {
                        let rest = current.split_off(at);
                        out.push(current.trim_end().to_owned());
                        current = rest;
                        used -= before;
                    }
                    None => {
                        out.push(std::mem::take(&mut current));
                        used = 0;
                    }
                }
            }
            current.push(character);
            used += columns;
            if character == ' ' && !current.trim().is_empty() {
                space = Some((current.len(), used));
            }
        }
        out.push(current);
    }
    out
}
