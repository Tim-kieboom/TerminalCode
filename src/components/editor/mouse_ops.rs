//! Mouse handling on the editor: click, drag, multi-click and the wheel.

use ratatui::layout::Position as ScreenPosition;
#[cfg(test)]
use ratatui::layout::Rect;
use unicode_segmentation::UnicodeSegmentation;

use crate::buffer::{BufferError, Position, Selection, Selections};
use crate::event::mouse::Clicks;

use super::Editor;
use super::motion::classify;
use super::text_layout::{column_at_display, display_column};

/// Lines one wheel notch scrolls.
const WHEEL_LINES: isize = 3;
/// Columns one horizontal wheel notch scrolls.
const WHEEL_COLUMNS: isize = 6;

/// What to do with a point outside the text area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outside {
    /// Ignore it (a click elsewhere is not for the text).
    Ignore,
    /// Treat it as the nearest edge of the text area (a drag that left it).
    Clamp,
}

impl Editor {
    #[cfg(test)]
    pub(crate) fn set_viewport(&mut self, text_area: Rect) {
        self.view.set_viewport(text_area);
    }

    /// Whether the cursor, the text or the viewport changed since the last
    /// call. The editor only scrolls to the cursor then, so that scrolling
    /// with the wheel is not undone by the next frame.
    #[cfg(test)]
    pub(crate) fn take_view_change(&mut self) -> bool {
        self.view.take_view_change(self.buffer.version())
    }

    /// A left button press at a screen cell. `extend` (Shift) moves the head
    /// of the selection instead of starting a new one. A press on a line
    /// number selects the line, and a drag from there goes on by whole lines.
    pub(crate) fn mouse_press(
        &mut self,
        column: u16,
        row: u16,
        extend: bool,
        clicks: Clicks,
    ) -> Result<(), BufferError> {
        self.view.drag_anchor = None;
        self.view.drag_lines = None;
        if let Some(line) = self.gutter_line_at(column, row) {
            return self.press_line_number(line, extend);
        }
        let Some(position) = self.position_at(column, row, Outside::Ignore)? else {
            return Ok(());
        };

        self.view.last_edit = None;
        let selection = match clicks {
            Clicks::Single if extend => {
                Selection::new(self.view.selections.primary().anchor(), position)
            }
            Clicks::Single => Selection::cursor(position),
            Clicks::Double => self.word_at(position)?,
            Clicks::Triple => self.line_at(position.line)?,
        };
        self.view.drag_anchor = Some(selection.anchor());
        self.view.selections = Selections::single(selection);
        Ok(())
    }

    /// The line whose number is at a screen cell, if the cell is on one.
    fn gutter_line_at(&self, column: u16, row: u16) -> Option<usize> {
        let (gutter, text) = (self.view.gutter?, self.view.text_area?);
        if !gutter.contains(ScreenPosition::new(column, row)) {
            return None;
        }
        let line = self.view.scroll.top + usize::from(row - text.y);
        (line < self.buffer.len_lines()).then_some(line)
    }

    fn press_line_number(&mut self, line: usize, extend: bool) -> Result<(), BufferError> {
        self.view.last_edit = None;
        let whole = self.line_at(line)?;
        let selection = match extend {
            // Shift keeps the old anchor and takes the line in.
            true => Selection::new(self.view.selections.primary().anchor(), whole.head()),
            false => whole,
        };
        self.view.drag_lines = Some(selection.anchor().line.min(line));
        self.view.selections = Selections::single(selection);
        Ok(())
    }

    /// The mouse moved with the button held: extends the selection from where
    /// the press started. Dragging outside the text selects up to its edge.
    pub(crate) fn mouse_drag(&mut self, column: u16, row: u16) -> Result<(), BufferError> {
        if let Some(first) = self.view.drag_lines {
            return self.drag_lines_to(first, column, row);
        }
        let Some(anchor) = self.view.drag_anchor else {
            return Ok(());
        };
        let Some(position) = self.position_at(column, row, Outside::Clamp)? else {
            return Ok(());
        };
        self.view.selections = Selections::single(Selection::new(anchor, position));
        Ok(())
    }

    /// Selects whole lines from `first` to the line under the pointer.
    fn drag_lines_to(&mut self, first: usize, column: u16, row: u16) -> Result<(), BufferError> {
        let Some(position) = self.position_at(column, row, Outside::Clamp)? else {
            return Ok(());
        };
        let (start, end) = (self.line_at(first)?, self.line_at(position.line)?);
        let selection = if position.line >= first {
            Selection::new(start.start(), end.end())
        } else {
            Selection::new(start.end(), end.start())
        };
        self.view.selections = Selections::single(selection);
        Ok(())
    }

    pub(crate) fn mouse_release(&mut self) {
        self.view.drag_anchor = None;
        self.view.drag_lines = None;
    }

    /// A right button press at a screen cell: the cursor goes there unless the
    /// press is inside the selection, which a context menu is about. False when
    /// the cell is not in the text.
    pub(crate) fn mouse_context(&mut self, column: u16, row: u16) -> Result<bool, BufferError> {
        let Some(position) = self.position_at(column, row, Outside::Ignore)? else {
            return Ok(false);
        };
        let selection = *self.view.selections.primary();
        let inside =
            !selection.is_empty() && selection.start() <= position && position < selection.end();
        if !inside {
            self.view.last_edit = None;
            self.view.selections = Selections::single(Selection::cursor(position));
        }
        Ok(true)
    }

    /// A middle button press at a screen cell: puts the cursor there. False
    /// when the cell is not in the text.
    pub(crate) fn mouse_place_cursor(
        &mut self,
        column: u16,
        row: u16,
    ) -> Result<bool, BufferError> {
        let Some(position) = self.position_at(column, row, Outside::Ignore)? else {
            return Ok(false);
        };
        self.view.last_edit = None;
        self.view.selections = Selections::single(Selection::cursor(position));
        Ok(true)
    }

    /// Scrolls by wheel notches (positive is down). The cursor stays where it
    /// is, even if it leaves the screen.
    pub(crate) fn scroll_lines(&mut self, notches: isize) {
        let last_line = (self.buffer.len_lines() - 1) as isize;
        let top = self.view.scroll.top as isize + notches * WHEEL_LINES;
        self.view.scroll.top = top.clamp(0, last_line) as usize;
    }

    /// Scrolls sideways by wheel notches (positive is right), never past the
    /// end of the longest visible line.
    pub(crate) fn scroll_columns(&mut self, notches: isize) {
        let visible = self.view.scroll.top
            ..(self.view.scroll.top + self.view.viewport_height).min(self.buffer.len_lines());
        let widest = visible
            .filter_map(|line| self.buffer.line_content(line).ok())
            .map(|content| display_column(&content, usize::MAX, self.view.tab_width()))
            .max()
            .unwrap_or(0);

        let left = self.view.scroll.left as isize + notches * WHEEL_COLUMNS;
        self.view.scroll.left = left.clamp(0, widest.saturating_sub(1) as isize) as usize;
    }

    /// The buffer position under a screen cell. Below the last line it is the
    /// end of the document; to the right of a line, that line's end.
    fn position_at(
        &self,
        column: u16,
        row: u16,
        outside: Outside,
    ) -> Result<Option<Position>, BufferError> {
        let Some(area) = self.view.text_area else {
            return Ok(None);
        };
        if area.width == 0 || area.height == 0 {
            return Ok(None);
        }
        if outside == Outside::Ignore && !area.contains(ScreenPosition::new(column, row)) {
            return Ok(None);
        }

        let row = row.clamp(area.y, area.y + area.height - 1);
        let column = column.max(area.x);
        let line = self.view.scroll.top + usize::from(row - area.y);

        let last_line = self.buffer.len_lines() - 1;
        if line > last_line {
            let end = self.buffer.line_len(last_line)?;
            return Ok(Some(Position::new(last_line, end)));
        }
        let content = self.buffer.line_content(line)?;
        let display = self.view.scroll.left + usize::from(column - area.x);
        Ok(Some(Position::new(
            line,
            column_at_display(&content, display, self.view.tab_width()),
        )))
    }

    /// The run of same-kind characters (word, punctuation or spaces) at
    /// `position`.
    fn word_at(&self, position: Position) -> Result<Selection, BufferError> {
        let content = self.buffer.line_content(position.line)?;
        let graphemes: Vec<&str> = content.graphemes(true).collect();
        let Some(last) = graphemes.len().checked_sub(1) else {
            return Ok(Selection::cursor(position));
        };

        let index = position.column.min(last);
        let class = classify(graphemes[index]);
        let mut start = index;
        while start > 0 && classify(graphemes[start - 1]) == class {
            start -= 1;
        }
        let mut end = index + 1;
        while end < graphemes.len() && classify(graphemes[end]) == class {
            end += 1;
        }
        Ok(Selection::new(
            Position::new(position.line, start),
            Position::new(position.line, end),
        ))
    }

    /// The whole line, including its line break when it has one.
    fn line_at(&self, line: usize) -> Result<Selection, BufferError> {
        let start = Position::new(line, 0);
        let end = if line + 1 < self.buffer.len_lines() {
            Position::new(line + 1, 0)
        } else {
            Position::new(line, self.buffer.line_len(line)?)
        };
        Ok(Selection::new(start, end))
    }
}
