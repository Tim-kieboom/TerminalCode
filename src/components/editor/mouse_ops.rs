//! Mouse handling on the editor: click, drag, multi-click and the wheel.

use ratatui::layout::{Position as ScreenPosition, Rect};
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
    /// Where the text is on screen, as last drawn.
    /// Where the text was last drawn.
    #[cfg(test)]
    pub(crate) fn text_area(&self) -> Option<Rect> {
        self.text_area
    }

    pub(crate) fn set_viewport(&mut self, text_area: Rect) {
        self.text_area = Some(text_area);
        self.viewport_height = usize::from(text_area.height);
    }

    /// Whether the cursor, the text or the viewport changed since the last
    /// call. The editor only scrolls to the cursor then, so that scrolling
    /// with the wheel is not undone by the next frame.
    pub(crate) fn take_view_change(&mut self) -> bool {
        let Some(area) = self.text_area else {
            return true;
        };
        let now = (
            self.selections.primary().head(),
            self.buffer.version(),
            area,
        );
        let changed = self.seen_view != Some(now);
        self.seen_view = Some(now);
        changed
    }

    /// A left button press at a screen cell. `extend` (Shift) moves the head
    /// of the selection instead of starting a new one.
    pub(crate) fn mouse_press(
        &mut self,
        column: u16,
        row: u16,
        extend: bool,
        clicks: Clicks,
    ) -> Result<(), BufferError> {
        self.drag_anchor = None;
        let Some(position) = self.position_at(column, row, Outside::Ignore)? else {
            return Ok(());
        };

        self.last_edit = None;
        let selection = match clicks {
            Clicks::Single if extend => {
                Selection::new(self.selections.primary().anchor(), position)
            }
            Clicks::Single => Selection::cursor(position),
            Clicks::Double => self.word_at(position)?,
            Clicks::Triple => self.line_at(position.line)?,
        };
        self.drag_anchor = Some(selection.anchor());
        self.selections = Selections::single(selection);
        Ok(())
    }

    /// The mouse moved with the button held: extends the selection from where
    /// the press started. Dragging outside the text selects up to its edge.
    pub(crate) fn mouse_drag(&mut self, column: u16, row: u16) -> Result<(), BufferError> {
        let Some(anchor) = self.drag_anchor else {
            return Ok(());
        };
        let Some(position) = self.position_at(column, row, Outside::Clamp)? else {
            return Ok(());
        };
        self.selections = Selections::single(Selection::new(anchor, position));
        Ok(())
    }

    pub(crate) fn mouse_release(&mut self) {
        self.drag_anchor = None;
    }

    /// Scrolls by wheel notches (positive is down). The cursor stays where it
    /// is, even if it leaves the screen.
    pub(crate) fn scroll_lines(&mut self, notches: isize) {
        let last_line = (self.buffer.len_lines() - 1) as isize;
        let top = self.scroll.top as isize + notches * WHEEL_LINES;
        self.scroll.top = top.clamp(0, last_line) as usize;
    }

    /// Scrolls sideways by wheel notches (positive is right), never past the
    /// end of the longest visible line.
    pub(crate) fn scroll_columns(&mut self, notches: isize) {
        let visible =
            self.scroll.top..(self.scroll.top + self.viewport_height).min(self.buffer.len_lines());
        let widest = visible
            .filter_map(|line| self.buffer.line_content(line).ok())
            .map(|content| display_column(&content, usize::MAX))
            .max()
            .unwrap_or(0);

        let left = self.scroll.left as isize + notches * WHEEL_COLUMNS;
        self.scroll.left = left.clamp(0, widest.saturating_sub(1) as isize) as usize;
    }

    /// The buffer position under a screen cell. Below the last line it is the
    /// end of the document; to the right of a line, that line's end.
    fn position_at(
        &self,
        column: u16,
        row: u16,
        outside: Outside,
    ) -> Result<Option<Position>, BufferError> {
        let Some(area) = self.text_area else {
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
        let line = self.scroll.top + usize::from(row - area.y);

        let last_line = self.buffer.len_lines() - 1;
        if line > last_line {
            let end = self.buffer.line_len(last_line)?;
            return Ok(Some(Position::new(last_line, end)));
        }
        let content = self.buffer.line_content(line)?;
        let display = self.scroll.left + usize::from(column - area.x);
        Ok(Some(Position::new(
            line,
            column_at_display(&content, display),
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
