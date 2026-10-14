//! The box of suggestions next to the cursor while typing: one row for each
//! item that matches (its kind as a letter, its name, and a dimmed note on the
//! right), the highlighted one marked, at most [`MOST_ROWS`] rows at a time.
//! It never takes the keyboard; the app steers it with a few keys.

use std::cell::{Cell, RefCell};

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ropey::Rope;
use unicode_width::UnicodeWidthStr;

use crate::components::diagnostic_popup::{place, wrap_text};
use crate::components::editor::render::fit;
use crate::components::scroll_text::TextScroll;
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
    /// Where the documentation of the highlighted item is scrolled to, and where
    /// it was drawn.
    docs_scroll: TextScroll,
    /// The item the documentation shown was of; another item starts at the top.
    docs_of: RefCell<String>,
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
            docs_scroll: TextScroll::default(),
            docs_of: RefCell::new(String::new()),
        }
    }

    /// The scrolling of the documentation box.
    pub(crate) fn docs_scroll(&self) -> &TextScroll {
        &self.docs_scroll
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
        crate::ui::backdrop::clear(frame, theme, area);
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

        let item = self.completion.selected_item();
        let docs = item.and_then(|item| item.documentation.as_deref());
        self.docs_scroll.not_drawn();
        if let (Some(item), Some(docs)) = (item, docs) {
            // Another item starts at the top.
            if *self.docs_of.borrow() != item.label {
                self.docs_scroll.reset();
                self.docs_of.replace(item.label.clone());
            }
            render_docs(frame, theme, docs, area, cursor, &self.docs_scroll);
        }
    }
}

/// The widest the box with the documentation gets, and the narrowest it gets
/// when the screen allows.
const DOCS_WIDEST: u16 = 64;
const DOCS_NARROWEST: u16 = 34;
/// How many lines of the documentation show at once; the rest scrolls.
const DOCS_LINES: usize = 8;

/// Draws what the server says about the highlighted item in a little box above
/// the cursor's line (above the list when the list is above the cursor, below
/// the line then), lined up with the list. Not drawn when there is no room.
fn render_docs(
    frame: &mut Frame,
    theme: &Theme,
    docs: &str,
    list: Rect,
    cursor: Position,
    scroll: &TextScroll,
) {
    let screen = frame.area();
    let width = list
        .width
        .clamp(DOCS_NARROWEST, DOCS_WIDEST)
        .min(screen.width);
    let text_width = usize::from(width.saturating_sub(4)).max(1);
    let wrapped = wrap_text(docs, text_width);

    // The box goes above the cursor's line, or above the list when the list is
    // above the cursor; the other side when that has no room. With room for
    // less than all of it, it shows what fits.
    let list_below_cursor = list.y > cursor.y;
    let (top_edge, bottom_edge) = match list_below_cursor {
        true => (cursor.y, list.bottom()),
        false => (list.y, cursor.y + 1),
    };
    let room_above = top_edge.saturating_sub(screen.y);
    let room_below = screen.bottom().saturating_sub(bottom_edge);
    let wanted = u16::try_from(wrapped.len().min(DOCS_LINES) + 2).unwrap_or(u16::MAX);
    let above = match (room_above >= wanted, room_below >= wanted) {
        (true, _) => true,
        (false, true) => false,
        (false, false) => room_above >= room_below,
    };
    let room = if above { room_above } else { room_below };
    if room < MIN_DOCS_ROWS {
        return;
    }
    let height = wanted.min(room);
    let y = if above {
        top_edge - height
    } else {
        bottom_edge
    };
    let rows = usize::from(height) - 2;
    let x = list
        .x
        .min(screen.right().saturating_sub(width))
        .max(screen.x);
    let area = Rect {
        x,
        y,
        width,
        height,
    };

    scroll.drawn_at(area);
    let first = scroll.fit(wrapped.len(), rows);
    let mut block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.style("pane.border"));
    if let Some(label) = scroll.label(wrapped.len(), rows) {
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
    let text: Vec<Line> = wrapped
        .into_iter()
        .skip(first)
        .take(rows)
        .map(Line::from)
        .collect();
    frame.render_widget(
        Paragraph::new(text).style(theme.style("completion.docs")),
        body,
    );
}

/// The fewest rows the box with the documentation is drawn in: the border and two lines.
const MIN_DOCS_ROWS: u16 = 4;
