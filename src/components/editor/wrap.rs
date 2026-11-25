//! Soft word wrap: how a line is cut into rows that fit the width of the text area, and
//! where a text position is on screen when lines have more than one row.
//!
//! Nothing here changes the text. A line is cut by [`wrap`] into [`Rows`] (the grapheme
//! where each row starts); [`Layout`] walks the rows from the top of the view, so only the
//! lines on screen are ever cut, and a cache keyed by the text of a line (not its number)
//! keeps the cuts of lines that did not change. A line longer than [`MAX_WRAPPED_BYTES`] is
//! not cut: it stays one row that scrolls sideways, as without wrap.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use unicode_segmentation::UnicodeSegmentation;

use crate::buffer::{Buffer, Position};

use super::text_layout::{Cell, TabWidth, column_at_display, display_column, width_at};
use super::view_state::ViewState;

/// A line longer than this many bytes is not wrapped (fixed, and says nothing about it).
pub(crate) const MAX_WRAPPED_BYTES: usize = 100_000;

/// How many cuts the cache keeps before it starts over.
const CACHE_LIMIT: usize = 2048;

/// How one line is cut into rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rows {
    /// The grapheme where each row starts; the first is 0.
    starts: Vec<usize>,
    /// How far the rows after the first are indented, in cells.
    indent: usize,
    /// The graphemes of the line.
    len: usize,
    /// False for a line that is left as one row (too long).
    wrapped: bool,
}

impl Rows {
    /// A line that is not cut: one row of `len` graphemes.
    pub(crate) fn unwrapped(len: usize) -> Self {
        Self {
            starts: vec![0],
            indent: 0,
            len,
            wrapped: false,
        }
    }

    pub(crate) fn is_wrapped(&self) -> bool {
        self.wrapped
    }

    pub(crate) fn count(&self) -> usize {
        self.starts.len()
    }

    pub(crate) fn start(&self, row: usize) -> usize {
        self.starts[row.min(self.starts.len() - 1)]
    }

    /// The grapheme after the last one of `row`.
    pub(crate) fn end(&self, row: usize) -> usize {
        self.starts.get(row + 1).copied().unwrap_or(self.len)
    }

    pub(crate) fn is_last(&self, row: usize) -> bool {
        row + 1 >= self.starts.len()
    }

    /// The cells the text of `row` is indented by.
    pub(crate) fn indent_of(&self, row: usize) -> usize {
        if row == 0 { 0 } else { self.indent }
    }

    /// The row that `column` is shown in. A column that starts a row belongs to it, unless
    /// `end_of_previous` says the cursor is drawn at the end of the row before.
    pub(crate) fn row_of(&self, column: usize, end_of_previous: bool) -> usize {
        let row = self
            .starts
            .partition_point(|&start| start <= column)
            .saturating_sub(1);
        if end_of_previous && row > 0 && column == self.starts[row] {
            row - 1
        } else {
            row
        }
    }
}

fn is_space(grapheme: &str) -> bool {
    grapheme.chars().next().is_some_and(char::is_whitespace)
}

/// Cuts `content` into rows for a text area `width` cells wide. A row is one cell narrower
/// than the area, so the cursor after its last character still has a cell. The cut is after
/// the last whitespace that fits (the whitespace stays at the end of the row), or in the
/// word when there is none; every row has at least one grapheme. The rows after the first
/// are indented like the line, by at most half the width.
pub(super) fn wrap(content: &str, width: usize, tab: TabWidth) -> Rows {
    let graphemes: Vec<&str> = content.graphemes(true).collect();
    let len = graphemes.len();
    let limit = width.saturating_sub(1).max(1);

    let mut indent = 0;
    let mut leading = 0;
    for grapheme in graphemes.iter().take_while(|g| matches!(**g, " " | "\t")) {
        indent += width_at(grapheme, indent, tab);
        leading += 1;
    }
    let indent = indent.min(limit / 2);

    let mut starts = vec![0];
    let mut row_start = 0;
    let mut column = 0;
    let mut space_end: Option<usize> = None;
    let mut index = 0;
    while index < len {
        let grapheme = graphemes[index];
        let cells = width_at(grapheme, column, tab);
        if column + cells > limit && index > row_start {
            let at = match space_end {
                Some(end) if end > row_start => end,
                _ => index,
            };
            starts.push(at);
            row_start = at;
            column = indent;
            space_end = None;
            for (offset, passed) in graphemes[at..index].iter().enumerate() {
                column += width_at(passed, column, tab);
                if is_space(passed) && at + offset >= leading {
                    space_end = Some(at + offset + 1);
                }
            }
            continue;
        }
        column += cells;
        // The indentation of the line is not a place to cut.
        if is_space(grapheme) && index >= leading {
            space_end = Some(index + 1);
        }
        index += 1;
    }
    Rows {
        starts,
        indent,
        len,
        wrapped: true,
    }
}

/// The cells of `row` of `content`, in the columns of the row (after its indentation).
pub(super) fn row_cells(content: &str, rows: &Rows, row: usize, tab: TabWidth) -> Vec<Cell> {
    let (start, end) = (rows.start(row), rows.end(row));
    let mut column = rows.indent_of(row);
    let mut cells = Vec::with_capacity(end - start);
    for (index, grapheme) in content
        .graphemes(true)
        .enumerate()
        .skip(start)
        .take(end - start)
    {
        let cell_width = width_at(grapheme, column, tab);
        column += cell_width;
        if cell_width == 0 {
            continue;
        }
        let text = if grapheme == "\t" {
            " ".repeat(cell_width)
        } else {
            grapheme.to_owned()
        };
        cells.push(Cell {
            text,
            grapheme: index,
        });
    }
    cells
}

/// The column, counted in cells from the left edge of the text area, where `column` of the
/// line is drawn when it is in `row`.
pub(crate) fn display_in_row(
    content: &str,
    rows: &Rows,
    row: usize,
    column: usize,
    tab: TabWidth,
) -> usize {
    let start = rows.start(row);
    let mut display = rows.indent_of(row);
    for grapheme in content
        .graphemes(true)
        .skip(start)
        .take(column.saturating_sub(start))
    {
        display += width_at(grapheme, display, tab);
    }
    display
}

/// The grapheme column nearest to the cell `display` of `row`, and whether it is the end of a
/// row that goes on (the cursor is then drawn at the end of this row, not at the start of
/// the next).
pub(crate) fn column_in_row(
    content: &str,
    rows: &Rows,
    row: usize,
    display: usize,
    tab: TabWidth,
) -> (usize, bool) {
    let (start, end) = (rows.start(row), rows.end(row));
    let mut column = rows.indent_of(row);
    for (index, grapheme) in content
        .graphemes(true)
        .enumerate()
        .skip(start)
        .take(end - start)
    {
        let cell_width = width_at(grapheme, column, tab);
        if display < column + cell_width.div_ceil(2) {
            return (index, false);
        }
        column += cell_width;
    }
    (end, !rows.is_last(row))
}

/// The cuts of the lines seen, by the text of the line and the width: the same line cut the
/// same way is not cut again, wherever it is in the file.
#[derive(Default)]
pub(super) struct WrapCache(RefCell<HashMap<u64, Arc<Rows>>>);

impl Clone for WrapCache {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl std::fmt::Debug for WrapCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WrapCache")
    }
}

impl WrapCache {
    fn rows(&self, content: &str, width: usize, tab: TabWidth) -> Arc<Rows> {
        let mut hasher = DefaultHasher::new();
        (content, width, tab.get()).hash(&mut hasher);
        let key = hasher.finish();
        if let Some(rows) = self.0.borrow().get(&key) {
            return Arc::clone(rows);
        }
        let rows = Arc::new(wrap(content, width, tab));
        let mut cache = self.0.borrow_mut();
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, Arc::clone(&rows));
        rows
    }
}

/// The affinity of the cursor: the position is drawn at the end of the row before it. It
/// lives for the operation that set it and the next one, so every other operation clears it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Affinity {
    position: Position,
    /// Set by the last operation; the next one reads it and ages it.
    fresh: bool,
}

impl ViewState {
    pub(crate) fn set_wrap(&mut self, on: bool) {
        if self.wrap != on {
            self.wrap = on;
            self.scroll.top_column = 0;
            self.affinity = None;
        }
    }

    /// Whether lines are wrapped here: the setting is on and the view is not a side of a
    /// diff (which stays unwrapped for now), and the width is known.
    pub(crate) fn is_wrapping(&self) -> bool {
        self.wrap && self.aligned.is_none() && self.wrap_width > 0
    }

    /// How `line` is cut, one row when wrap is off or the line is too long.
    pub(super) fn rows(&self, buffer: &Buffer, line: usize) -> Arc<Rows> {
        if !self.is_wrapping() {
            return Arc::new(Rows::unwrapped(buffer.line_len(line).unwrap_or(0)));
        }
        let Ok(content) = buffer.line_content(line) else {
            return Arc::new(Rows::unwrapped(0));
        };
        if content.len() > MAX_WRAPPED_BYTES {
            return Arc::new(Rows::unwrapped(buffer.line_len(line).unwrap_or(0)));
        }
        self.wraps.rows(&content, self.wrap_width, self.tab_width)
    }

    /// Whether the cursor at `head` is drawn at the end of the row before it.
    pub(crate) fn affinity_at(&self, head: Position) -> bool {
        self.affinity.is_some_and(|a| a.position == head)
    }

    /// The cursor at `position` is drawn at the end of the row before it (or, with `None`,
    /// it is not).
    pub(super) fn set_affinity(&mut self, position: Option<Position>) {
        self.affinity = position.map(|position| Affinity {
            position,
            fresh: true,
        });
    }

    /// Called before every operation on the editor: what the last operation set is kept
    /// for this one to read, and what an earlier one set is dropped.
    pub(crate) fn age_affinity(&mut self) {
        match &mut self.affinity {
            Some(affinity) if affinity.fresh => affinity.fresh = false,
            _ => self.affinity = None,
        }
    }
}

impl ViewState {
    /// With wrap, makes the top of the view the start of a row of the top line, which is
    /// what it was set to when the width was another one.
    pub(crate) fn normalize_top(&mut self, buffer: &Buffer) {
        if !self.is_wrapping() {
            self.scroll.top_column = 0;
            return;
        }
        let rows = self.rows(buffer, self.scroll.top);
        let row = rows.row_of(self.scroll.top_column, false);
        self.scroll.top_column = rows.start(row);
    }
}

/// A row on screen.
#[derive(Debug, Clone)]
pub(crate) struct Slot {
    pub(super) line: usize,
    pub(super) row: usize,
    pub(super) rows: Arc<Rows>,
}

impl Slot {
    pub(super) fn end(&self) -> usize {
        self.rows.end(self.row)
    }
}

/// The rows of a view, from its top.
pub(crate) struct Layout<'a> {
    pub(super) buffer: &'a Buffer,
    pub(super) view: &'a ViewState,
}

impl<'a> Layout<'a> {
    pub(super) fn new(buffer: &'a Buffer, view: &'a ViewState) -> Self {
        Self { buffer, view }
    }

    fn slot(&self, line: usize, row: usize) -> Slot {
        Slot {
            line,
            row,
            rows: self.view.rows(self.buffer, line),
        }
    }

    /// The line and row at the top of the text.
    pub(super) fn top(&self) -> (usize, usize) {
        let line = self.view.scroll.top;
        let rows = self.view.rows(self.buffer, line);
        (line, rows.row_of(self.view.scroll.top_column, false))
    }

    /// The row after (or before) `row` of `line`, over the line breaks.
    pub(super) fn step(&self, line: usize, row: usize, forward: bool) -> Option<(usize, usize)> {
        if forward {
            let rows = self.view.rows(self.buffer, line);
            if !rows.is_last(row) {
                return Some((line, row + 1));
            }
            return (line + 1 < self.buffer.len_lines()).then_some((line + 1, 0));
        }
        if row > 0 {
            return Some((line, row - 1));
        }
        let line = line.checked_sub(1)?;
        Some((line, self.view.rows(self.buffer, line).count() - 1))
    }

    /// The rows on screen, `height` of them at most, fewer at the end of the text.
    pub(super) fn slots(&self, height: usize) -> Vec<Slot> {
        let mut slots = Vec::with_capacity(height);
        let (mut line, mut row) = self.top();
        if line >= self.buffer.len_lines() {
            return slots;
        }
        while slots.len() < height {
            slots.push(self.slot(line, row));
            match self.step(line, row, true) {
                Some(next) => (line, row) = next,
                None => break,
            }
        }
        slots
    }

    /// How many rows below the top of the text the row of `position` is, when that is less
    /// than `height`.
    pub(super) fn row_offset(
        &self,
        position: Position,
        end_of_previous: bool,
        height: usize,
    ) -> Option<usize> {
        let (top_line, top_row) = self.top();
        if position.line < top_line || position.line >= top_line + height {
            return None;
        }
        let row = self
            .view
            .rows(self.buffer, position.line)
            .row_of(position.column, end_of_previous);
        let (mut line, mut at) = (top_line, top_row);
        for offset in 0..height {
            if (line, at) == (position.line, row) {
                return Some(offset);
            }
            (line, at) = self.step(line, at, true)?;
        }
        None
    }

    /// Where `position` is in the text area as (row, cell), when it is on screen.
    pub(super) fn cell_of(
        &self,
        position: Position,
        end_of_previous: bool,
        height: usize,
    ) -> Option<(usize, usize)> {
        let offset = self.row_offset(position, end_of_previous, height)?;
        let rows = self.view.rows(self.buffer, position.line);
        let content = self.buffer.line_content(position.line).ok()?;
        let tab = self.view.tab_width;
        if !rows.is_wrapped() {
            let display = display_column(&content, position.column, tab);
            return Some((offset, display.checked_sub(self.view.scroll.left)?));
        }
        let row = rows.row_of(position.column, end_of_previous);
        Some((
            offset,
            display_in_row(&content, &rows, row, position.column, tab),
        ))
    }

    /// The position under cell `display` of the screen row `offset` rows below the top, and
    /// whether it is drawn at the end of its row. Below the last row it is the end of the
    /// text.
    pub(super) fn position_at(&self, offset: usize, display: usize) -> (Position, bool) {
        let (mut line, mut row) = self.top();
        for _ in 0..offset {
            match self.step(line, row, true) {
                Some(next) => (line, row) = next,
                None => {
                    let last = self.buffer.len_lines() - 1;
                    let end = self.buffer.line_len(last).unwrap_or(0);
                    return (Position::new(last, end), false);
                }
            }
        }
        let rows = self.view.rows(self.buffer, line);
        let content = self.buffer.line_content(line).unwrap_or_default();
        let tab = self.view.tab_width;
        if !rows.is_wrapped() {
            let at = column_at_display(&content, display + self.view.scroll.left, tab);
            return (Position::new(line, at), false);
        }
        let (column, at_end) = column_in_row(&content, &rows, row, display, tab);
        (Position::new(line, column), at_end)
    }
}

/// Scrolls the minimum needed to bring the row of `position` into a viewport of `height`
/// rows and `width` cells. The rows are counted only between the top and the cursor, never
/// the whole text.
pub(super) fn scroll_to_show(
    buffer: &Buffer,
    view: &mut ViewState,
    position: Position,
    end_of_previous: bool,
    height: usize,
    width: usize,
) {
    view.viewport_height = height;
    if height == 0 {
        return;
    }
    let layout = Layout::new(buffer, view);
    let rows = view.rows(buffer, position.line);
    let row = rows.row_of(position.column, end_of_previous);
    let (top_line, top_row) = layout.top();
    let above = position.line < top_line || (position.line == top_line && row < top_row);
    let new_top = if above {
        Some((position.line, row))
    } else if layout
        .row_offset(position, end_of_previous, height)
        .is_none()
    {
        // Below: the cursor's row becomes the last row of the view.
        let (mut line, mut at) = (position.line, row);
        for _ in 1..height {
            match layout.step(line, at, false) {
                Some(previous) => (line, at) = previous,
                None => break,
            }
        }
        Some((line, at))
    } else {
        None
    };
    if let Some((line, at)) = new_top {
        let start = view.rows(buffer, line).start(at);
        view.scroll.top = line;
        view.scroll.top_column = start;
    }
    view.scroll.left = match rows.is_wrapped() {
        true => 0,
        false => {
            let content = buffer.line_content(position.line).unwrap_or_default();
            let display = display_column(&content, position.column, view.tab_width);
            super::scroll_axis(view.scroll.left, display, width)
        }
    };
}
