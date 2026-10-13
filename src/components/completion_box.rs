//! The box of suggestions next to the cursor while typing: one row for each
//! item that matches (its kind as a letter, its name, and a dimmed note on the
//! right), the highlighted one marked, at most [`MOST_ROWS`] rows at a time.
//! It never takes the keyboard; the app steers it with a few keys.

use std::cell::Cell;

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ropey::Rope;
use unicode_width::UnicodeWidthStr;

use crate::components::diagnostic_popup::place;
use crate::components::editor::render::fit;
use crate::components::workspace::DocumentId;
use crate::lsp::Completion;
use crate::ui::theme::Theme;

/// How many suggestions show at once; the rest scroll.
const MOST_ROWS: usize = 8;
/// The box is at least this many columns of rows wide, and at most [`WIDEST`].
const NARROWEST: usize = 20;
const WIDEST: usize = 64;
/// Columns between the left edge of the box and the start of a name: the
/// border, a space, the kind letter and a space.
const BEFORE_NAME: u16 = 4;

/// The suggestions for the word being typed in one document.
#[derive(Debug)]
pub(crate) struct CompletionBox {
    completion: Completion,
    /// The document the word is in; the box is gone when another is in front.
    document: DocumentId,
    /// The text as it was when the box last looked at it, to find what changed.
    text: Rope,
    /// Opened by a trigger character such as `.`, so that it may stay open with
    /// nothing typed yet; a box that is not closes when the word is empty again.
    by_trigger: bool,
    /// The first row shown.
    scroll: Cell<usize>,
}

impl CompletionBox {
    pub(crate) fn new(
        completion: Completion,
        document: DocumentId,
        text: Rope,
        by_trigger: bool,
    ) -> Self {
        Self {
            completion,
            document,
            text,
            by_trigger,
            scroll: Cell::new(0),
        }
    }

    pub(crate) fn completion(&self) -> &Completion {
        &self.completion
    }

    pub(crate) fn completion_mut(&mut self) -> &mut Completion {
        &mut self.completion
    }

    pub(crate) fn document(&self) -> DocumentId {
        self.document
    }

    pub(crate) fn text(&self) -> &Rope {
        &self.text
    }

    pub(crate) fn set_text(&mut self, text: Rope) {
        self.text = text;
    }

    pub(crate) fn by_trigger(&self) -> bool {
        self.by_trigger
    }

    /// The rows of the box as text, for a box `width` columns wide inside its
    /// border: the kind letter, the name, and the note at the right edge. Only
    /// the rows that show.
    fn rows(&self, width: usize) -> Vec<(String, String, String)> {
        let selected = self.completion.selected_index();
        let first = self.first_row(selected);
        self.completion
            .items()
            .enumerate()
            .skip(first)
            .take(MOST_ROWS)
            .map(|(_, item)| {
                let detail = item.detail.as_deref().unwrap_or("");
                let name_width = width.saturating_sub(4);
                let name = fit(&item.label, name_width);
                let room = name_width.saturating_sub(name.width());
                // The note gets what the name leaves, less a gap of two.
                let note = match room > 3 {
                    true => fit(detail, room - 2),
                    false => String::new(),
                };
                (item.kind.tag().to_string(), name, note)
            })
            .collect()
    }

    /// The first row to show so that the highlighted one is among them.
    fn first_row(&self, selected: usize) -> usize {
        let mut first = self.scroll.get();
        if selected < first {
            first = selected;
        } else if selected >= first + MOST_ROWS {
            first = selected + 1 - MOST_ROWS;
        }
        first = first.min(self.completion.len().saturating_sub(MOST_ROWS));
        self.scroll.set(first);
        first
    }

    /// The width of the rows inside the border, from the longest of them.
    fn row_width(&self) -> usize {
        self.completion
            .items()
            .map(|item| {
                let detail = item
                    .detail
                    .as_deref()
                    .map_or(0, |detail| detail.width() + 2);
                4 + item.label.width() + detail
            })
            .max()
            .unwrap_or(0)
            .clamp(NARROWEST, WIDEST)
    }

    /// Draws the box under the word (above it when there is no room), with the
    /// cursor at `cursor` on the screen. The row of the highlighted item is
    /// marked.
    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme, cursor: Position) {
        if self.completion.is_empty() {
            return;
        }
        let screen = frame.area();
        let columns = self
            .row_width()
            .min(usize::from(screen.width.saturating_sub(2)))
            .max(1);
        let rows = self.completion.len().min(MOST_ROWS);
        let width = u16::try_from(columns).unwrap_or(u16::MAX) + 2;
        let height = u16::try_from(rows).unwrap_or(u16::MAX) + 2;
        // The names line up with the word being typed.
        let typed = u16::try_from(self.completion.typed().width()).unwrap_or(u16::MAX);
        let word_x = cursor.x.saturating_sub(typed);
        let anchor = Position::new(word_x.saturating_sub(BEFORE_NAME), cursor.y);
        let area = place(screen, anchor, width, height);

        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border.focused"));
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);

        let first = self.scroll.get().min(self.completion.len() - 1);
        let selected = self.completion.selected_index();
        let item_style = Style::default();
        let lines: Vec<Line> = self
            .rows(usize::from(inner.width))
            .into_iter()
            .enumerate()
            .map(|(offset, (tag, name, note))| {
                let marked = first + offset == selected;
                let used = 3 + name.width() + note.width();
                let gap = usize::from(inner.width).saturating_sub(used + 1);
                let line = Line::from(vec![
                    Span::raw(" "),
                    Span::styled(tag, theme.style("completion.kind")),
                    Span::raw(" "),
                    Span::styled(name, item_style),
                    Span::raw(" ".repeat(gap)),
                    Span::styled(note, theme.style("completion.detail")),
                ]);
                match marked {
                    true => line.style(theme.style("completion.selected")),
                    false => line,
                }
            })
            .collect();
        frame.render_widget(
            Paragraph::new(lines),
            Rect {
                height: rows as u16,
                ..inner
            },
        );
    }
}
