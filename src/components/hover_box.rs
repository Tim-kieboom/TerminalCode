//! What a language server says about the thing under the mouse pointer or the
//! cursor (its type, its signature, its documentation), in a small box next to
//! it. It never takes the keyboard. A box that came from the pointer goes away
//! with a key, a click, a scroll, an edit, or the pointer moving to another
//! word; one that was pinned with `f6` stays until a key or a click.
//! A text that is longer than the box scrolls: the wheel over it, or the arrow
//! and page keys while it is pinned.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::components::diagnostic_popup::{place, wrap_text};
use crate::components::scroll_text::TextScroll;
use crate::components::workspace::DocumentId;
use crate::ui::theme::Theme;

/// The widest the box gets, and the narrowest when the screen allows.
const WIDEST: u16 = 80;
const NARROWEST: u16 = 20;
/// How many lines show at once; the rest scrolls.
const MOST_LINES: usize = 14;
/// How many lines of a text are kept, however long it is.
const KEPT_LINES: usize = 500;

#[derive(Debug)]
pub(crate) struct HoverBox {
    text: String,
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
        anchor: Position,
        document: DocumentId,
        version: u64,
        pinned: bool,
    ) -> Self {
        Self {
            text,
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

    pub(crate) fn text(&self) -> &str {
        &self.text
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
            .map(UnicodeWidthStr::width)
            .max()
            .unwrap_or(0);
        let width = u16::try_from(longest + 4)
            .unwrap_or(u16::MAX)
            .clamp(NARROWEST, WIDEST)
            .min(screen.width);
        let text_width = usize::from(width.saturating_sub(4)).max(1);
        let mut lines = wrap_text(&self.text, text_width);
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
        frame.render_widget(Clear, area);
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
            .map(Line::from)
            .collect();
        frame.render_widget(Paragraph::new(shown).style(theme.style("hover.text")), body);
    }
}
