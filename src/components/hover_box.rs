//! What a language server says about the thing under the mouse pointer or the
//! cursor (its type, its signature, its documentation), in a small box next to
//! it. It never takes the keyboard. A box that came from the pointer goes away
//! with a key, a click, a scroll, an edit, or the pointer moving to another
//! word; one that was pinned with `f6` stays until a key or a click.
//! A text that is longer than the box scrolls: the wheel over it, or the arrow
//! and page keys while it is pinned.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::components::editor::render::slot_of;
use crate::components::popups::diagnostic_popup::{Entry, place, wrap_text};
use crate::components::scroll_text::TextScroll;
use crate::ids::DocumentId;
use crate::lsp::Severity;
use crate::ui::theme::Theme;

/// The widest the box gets, and the narrowest when the screen allows.
const WIDEST: u16 = 80;
const NARROWEST: u16 = 20;
/// How many lines show at once; the rest scrolls.
const MOST_LINES: usize = 14;
/// How many lines of a text are kept, however long it is.
const KEPT_LINES: usize = 500;

/// What a line of the box is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Header(Severity),
    Rule,
    Text,
}

#[derive(Debug)]
pub(crate) struct HoverBox {
    /// What the server says about the thing; empty when it says nothing.
    text: String,
    /// The diagnostics whose range holds the thing, worst first. They come before
    /// the text, with a rule between.
    problems: Vec<Entry>,
    /// The cell the box hangs from: under the pointer, or under the cursor.
    anchor: Position,
    /// What the text was about, so that an edit or another tab takes it away.
    document: DocumentId,
    version: u64,
    /// Stays when the pointer moves: asked for with `f6`.
    pinned: bool,
    scroll: TextScroll,
}

impl HoverBox {
    pub(crate) fn new(
        text: String,
        problems: Vec<Entry>,
        anchor: Position,
        document: DocumentId,
        version: u64,
        pinned: bool,
    ) -> Self {
        Self {
            text,
            problems,
            anchor,
            document,
            version,
            pinned,
            scroll: TextScroll::default(),
        }
    }

    pub(crate) fn document(&self) -> DocumentId {
        self.document
    }

    pub(crate) fn version(&self) -> u64 {
        self.version
    }

    #[cfg(test)]
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    #[cfg(test)]
    pub(crate) fn problems(&self) -> &[Entry] {
        &self.problems
    }

    /// The lines as drawn for `width` columns, as plain text.
    #[cfg(test)]
    pub(crate) fn drawn_lines(&self, width: usize) -> Vec<String> {
        self.lines(width)
            .into_iter()
            .map(|(_, text)| text)
            .collect()
    }

    /// The text as it is drawn for `width` columns: the problems, a rule, the
    /// documentation.
    fn lines(&self, width: usize) -> Vec<(Kind, String)> {
        let mut lines = Vec::new();
        for (index, entry) in self.problems.iter().enumerate() {
            if index > 0 {
                lines.push((Kind::Text, String::new()));
            }
            lines.push((Kind::Header(entry.severity), entry.header.clone()));
            lines.extend(
                wrap_text(&entry.message, width)
                    .into_iter()
                    .map(|text| (Kind::Text, text)),
            );
        }
        if !self.problems.is_empty() && !self.text.is_empty() {
            lines.push((Kind::Rule, "─".repeat(width)));
        }
        lines.extend(
            wrap_text(&self.text, width)
                .into_iter()
                .map(|text| (Kind::Text, text)),
        );
        lines
    }

    pub(crate) fn is_pinned(&self) -> bool {
        self.pinned
    }

    /// Makes the box stay until a key or a click.
    pub(crate) fn pin(&mut self) {
        self.pinned = true;
    }

    pub(crate) fn scroll(&self) -> &TextScroll {
        &self.scroll
    }

    /// Draws the box under its anchor, above it when there is no room below.
    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        let screen = frame.area();
        // As wide as the longest line, within what the screen allows.
        let longest = self
            .text
            .lines()
            .chain(
                self.problems
                    .iter()
                    .flat_map(|entry| entry.message.lines().chain([entry.header.as_str()])),
            )
            .map(UnicodeWidthStr::width)
            .max()
            .unwrap_or(0);
        let width = u16::try_from(longest + 4)
            .unwrap_or(u16::MAX)
            .clamp(NARROWEST, WIDEST)
            .min(screen.width);
        let text_width = usize::from(width.saturating_sub(4)).max(1);
        let mut lines = self.lines(text_width);
        lines.truncate(KEPT_LINES);
        let rows = lines
            .len()
            .min(MOST_LINES)
            .min(usize::from(screen.height.saturating_sub(2)));
        let height = u16::try_from(rows).unwrap_or(u16::MAX) + 2;
        if rows == 0 || width < 5 {
            self.scroll.not_drawn();
            return;
        }

        let area = place(screen, self.anchor, width, height);
        self.scroll.drawn_at(area);
        let first = self.scroll.fit(lines.len(), rows);
        let mut block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border"));
        if let Some(label) = self.scroll.label(lines.len(), rows) {
            block = block.title_bottom(Line::from(label).right_aligned());
        }
        let inner = block.inner(area);
        crate::ui::backdrop::clear(frame, theme, area);
        frame.render_widget(block, area);
        let body = Rect {
            x: inner.x + 1,
            width: inner.width.saturating_sub(2),
            ..inner
        };
        let shown: Vec<Line> = lines
            .into_iter()
            .skip(first)
            .take(rows)
            .map(|(kind, text)| match kind {
                Kind::Header(severity) => Line::from(Span::styled(
                    text,
                    theme.style(slot_of(severity)).add_modifier(Modifier::BOLD),
                )),
                Kind::Rule => Line::from(Span::styled(text, theme.style("pane.border"))),
                Kind::Text => Line::from(text),
            })
            .collect();
        frame.render_widget(Paragraph::new(shown).style(theme.style("hover.text")), body);
    }
}
