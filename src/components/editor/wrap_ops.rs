//! What the editor does differently while lines are wrapped: up and down move by row,
//! `home` and `end` go to the ends of the row first, and the wheel and the page keys scroll
//! by rows. Only for one cursor; with several cursors the motions go by line as before.

use crate::buffer::{BufferError, Position, Selection};

use super::Editor;
use super::motion::{Motion, Target};
use super::text_layout::{column_at_display, display_column};
use super::wrap::{Layout, Rows, column_in_row, display_in_row};

impl Editor {
    /// Where `motion` goes with wrapped lines, when that is not what it does without them.
    pub(super) fn wrapped_target(
        &self,
        selection: &Selection,
        motion: Motion,
    ) -> Result<Option<Target>, BufferError> {
        match motion {
            Motion::Up => self.rows_vertically(selection, false, 1).map(Some),
            Motion::Down => self.rows_vertically(selection, true, 1).map(Some),
            Motion::PageUp => self
                .rows_vertically(selection, false, self.page_lines())
                .map(Some),
            Motion::PageDown => self
                .rows_vertically(selection, true, self.page_lines())
                .map(Some),
            Motion::LineStart => self.row_start(selection),
            Motion::LineEnd => self.row_end(selection),
            _ => Ok(None),
        }
    }

    /// The display column of `column` in its row, counted from the left edge of the text.
    fn display_of(&self, content: &str, rows: &Rows, row: usize, column: usize) -> usize {
        match rows.is_wrapped() {
            true => display_in_row(content, rows, row, column, self.view.tab_width()),
            false => display_column(content, column, self.view.tab_width()),
        }
    }

    /// `steps` rows up or down, aiming for the display column the cursor started in.
    fn rows_vertically(
        &self,
        selection: &Selection,
        forward: bool,
        steps: usize,
    ) -> Result<Target, BufferError> {
        let head = selection.head();
        let rows = self.view.rows(&self.buffer, head.line);
        let row = rows.row_of(head.column, self.view.affinity_at(head));
        let content = self.buffer.line_content(head.line)?;
        let wanted = match selection.desired_column() {
            Some(wanted) => wanted,
            None => self.display_of(&content, &rows, row, head.column),
        };

        let layout = Layout::new(&self.buffer, &self.view);
        let mut at = (head.line, row);
        let mut moved = 0;
        for _ in 0..steps {
            match layout.step(at.0, at.1, forward) {
                Some(next) => {
                    at = next;
                    moved += 1;
                }
                None => break,
            }
        }
        if moved == 0 {
            let last = self.buffer.len_lines() - 1;
            return Ok(match forward {
                true => Target::at(last, self.buffer.line_len(last)?),
                false => Target::at(0, 0),
            });
        }

        let (line, row) = at;
        let rows = self.view.rows(&self.buffer, line);
        let content = self.buffer.line_content(line)?;
        let tab = self.view.tab_width();
        let (column, at_end) = match rows.is_wrapped() {
            true => column_in_row(&content, &rows, row, wanted, tab),
            false => (column_at_display(&content, wanted, tab), false),
        };
        Ok(Target {
            position: Position::new(line, column),
            desired_column: Some(wanted),
            affinity: at_end,
        })
    }

    /// `home`: the start of the row; from the start of a row, the start of the line.
    fn row_start(&self, selection: &Selection) -> Result<Option<Target>, BufferError> {
        let head = selection.head();
        let rows = self.view.rows(&self.buffer, head.line);
        if !rows.is_wrapped() {
            return Ok(None);
        }
        let end_of_previous = self.view.affinity_at(head);
        let row = rows.row_of(head.column, end_of_previous);
        let start = rows.start(row);
        let there = head.column == start && !end_of_previous;
        Ok(Some(Target::at(head.line, if there { 0 } else { start })))
    }

    /// `end`: the end of the row, drawn at the end of that row; from there, the end of the
    /// line. On the last row it is the end of the line.
    fn row_end(&self, selection: &Selection) -> Result<Option<Target>, BufferError> {
        let head = selection.head();
        let rows = self.view.rows(&self.buffer, head.line);
        if !rows.is_wrapped() {
            return Ok(None);
        }
        let end_of_previous = self.view.affinity_at(head);
        let row = rows.row_of(head.column, end_of_previous);
        if rows.is_last(row) {
            return Ok(None);
        }
        let end = rows.end(row);
        if head.column == end && end_of_previous {
            return Ok(Some(Target::at(
                head.line,
                self.buffer.line_len(head.line)?,
            )));
        }
        Ok(Some(Target {
            position: Position::new(head.line, end),
            desired_column: None,
            affinity: true,
        }))
    }

    /// Moves the top of the view by `delta` rows (down when positive), as far as there are
    /// rows. The cursor stays where it is.
    pub(crate) fn scroll_rows(&mut self, delta: isize) {
        let layout = Layout::new(&self.buffer, &self.view);
        let (mut line, mut row) = layout.top();
        for _ in 0..delta.unsigned_abs() {
            match layout.step(line, row, delta > 0) {
                Some(next) => (line, row) = next,
                None => break,
            }
        }
        let start = self.view.rows(&self.buffer, line).start(row);
        self.view.scroll.top = line;
        self.view.scroll.top_column = start;
    }

    /// After a page motion the view moves by the rows of a page, so the cursor keeps its
    /// row on screen.
    pub(super) fn follow_page_rows(&mut self, motion: Motion) {
        let page = self.page_lines() as isize;
        match motion {
            Motion::PageUp => self.scroll_rows(-page),
            Motion::PageDown => self.scroll_rows(page),
            _ => {}
        }
    }
}
