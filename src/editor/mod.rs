use std::ops::Range;

use crate::buffer::{Buffer, BufferError, Edit, FileError, Position, Selection, Selections};

pub(crate) use motion::Motion;

mod motion;
#[cfg(test)]
mod tests;

/// First visible line and display column of the editor viewport.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Scroll {
    pub(crate) top: usize,
    pub(crate) left: usize,
}

/// One open document with its selection and viewport.
///
/// Only the primary selection is edited for now; multi-cursor editing comes
/// after 0.1.0.
#[derive(Debug, Default)]
pub(crate) struct Editor {
    buffer: Buffer,
    selections: Selections,
    scroll: Scroll,
}

impl Editor {
    pub(crate) fn new(buffer: Buffer) -> Self {
        Self {
            buffer,
            selections: Selections::default(),
            scroll: Scroll::default(),
        }
    }

    pub(crate) fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub(crate) fn selections(&self) -> &Selections {
        &self.selections
    }

    pub(crate) fn scroll(&self) -> Scroll {
        self.scroll
    }

    /// File name for titles and the status bar, with `[+]` when modified.
    pub(crate) fn display_name(&self) -> String {
        let name = self
            .buffer
            .path()
            .and_then(|path| path.file_name())
            .map_or_else(|| "[no name]".into(), |name| name.to_string_lossy());
        match self.buffer.is_dirty() {
            true => format!("{name} [+]"),
            false => name.into_owned(),
        }
    }

    /// Moves the cursor. With a selection, left and right collapse it to its
    /// start and end instead of moving.
    pub(crate) fn move_cursor(&mut self, motion: Motion) -> Result<(), BufferError> {
        let selection = *self.selections.primary();
        let collapse_to = match motion {
            Motion::Left if !selection.is_empty() => Some(selection.start()),
            Motion::Right if !selection.is_empty() => Some(selection.end()),
            _ => None,
        };
        let moved = match collapse_to {
            Some(position) => Selection::cursor(position),
            None => {
                let target = self.target(&selection, motion)?;
                Selection::cursor(target.position).with_desired_column(target.desired_column)
            }
        };
        self.selections = Selections::single(moved);
        Ok(())
    }

    /// Moves the head of the selection, keeping the anchor.
    pub(crate) fn select(&mut self, motion: Motion) -> Result<(), BufferError> {
        let selection = *self.selections.primary();
        let target = self.target(&selection, motion)?;
        let extended = Selection::new(selection.anchor(), target.position)
            .with_desired_column(target.desired_column);
        self.selections = Selections::single(extended);
        Ok(())
    }

    /// Replaces the selection (or inserts at the cursor) with `text`.
    pub(crate) fn insert_text(&mut self, text: &str) -> Result<(), BufferError> {
        let range = self.selection_bytes()?;
        if range.is_empty() && text.is_empty() {
            return Ok(());
        }
        self.replace(range, text)
    }

    /// Inserts a line break in the file's own style.
    pub(crate) fn insert_newline(&mut self) -> Result<(), BufferError> {
        let line_break = self.buffer.line_ending().as_str();
        self.insert_text(line_break)
    }

    /// Deletes the selection, or the grapheme before the cursor. At the start
    /// of a line it joins the line with the previous one.
    pub(crate) fn delete_backward(&mut self) -> Result<(), BufferError> {
        let selection = *self.selections.primary();
        if !selection.is_empty() {
            return self.replace(self.selection_bytes()?, "");
        }

        let head = selection.head();
        let start = if head.column > 0 {
            Position::new(head.line, head.column - 1)
        } else if let Some(line) = head.line.checked_sub(1) {
            Position::new(line, self.buffer.line_len(line)?)
        } else {
            return Ok(());
        };
        self.replace_between(start, head, "")
    }

    /// Deletes the selection, or the grapheme after the cursor. At the end of
    /// a line it joins the next line onto it.
    pub(crate) fn delete_forward(&mut self) -> Result<(), BufferError> {
        let selection = *self.selections.primary();
        if !selection.is_empty() {
            return self.replace(self.selection_bytes()?, "");
        }

        let head = selection.head();
        let end = if head.column < self.buffer.line_len(head.line)? {
            Position::new(head.line, head.column + 1)
        } else if head.line + 1 < self.buffer.len_lines() {
            Position::new(head.line + 1, 0)
        } else {
            return Ok(());
        };
        self.replace_between(head, end, "")
    }

    pub(crate) fn undo(&mut self) -> Result<(), BufferError> {
        if let Some(selections) = self.buffer.undo()? {
            self.selections = selections;
        }
        Ok(())
    }

    pub(crate) fn redo(&mut self) -> Result<(), BufferError> {
        if let Some(selections) = self.buffer.redo()? {
            self.selections = selections;
        }
        Ok(())
    }

    pub(crate) fn save(&mut self) -> Result<(), FileError> {
        self.buffer.save()
    }

    /// Scrolls the minimum needed to bring `line` and `display_column` into a
    /// viewport of `height` by `width` cells.
    pub(crate) fn scroll_to_show(
        &mut self,
        line: usize,
        display_column: usize,
        height: usize,
        width: usize,
    ) {
        self.scroll.top = scroll_axis(self.scroll.top, line, height);
        self.scroll.left = scroll_axis(self.scroll.left, display_column, width);
    }

    fn target(&self, selection: &Selection, motion: Motion) -> Result<motion::Target, BufferError> {
        motion::target(
            &self.buffer,
            selection.head(),
            selection.desired_column(),
            motion,
        )
    }

    fn selection_bytes(&self) -> Result<Range<usize>, BufferError> {
        let selection = self.selections.primary();
        let start = self.buffer.position_to_byte(selection.start())?;
        let end = self.buffer.position_to_byte(selection.end())?;
        Ok(start..end)
    }

    fn replace_between(
        &mut self,
        start: Position,
        end: Position,
        text: &str,
    ) -> Result<(), BufferError> {
        let range = self.buffer.position_to_byte(start)?..self.buffer.position_to_byte(end)?;
        self.replace(range, text)
    }

    /// One undoable edit that leaves the cursor just after `text`.
    fn replace(&mut self, range: Range<usize>, text: &str) -> Result<(), BufferError> {
        let mut transaction = self.buffer.begin_transaction(self.selections.clone());
        transaction.apply(&Edit::new(range.clone(), text))?;
        let cursor = transaction
            .buffer()
            .byte_to_position(range.start + text.len())?;

        let after = Selections::single(Selection::cursor(cursor));
        transaction.commit(after.clone());
        self.selections = after;
        Ok(())
    }
}

/// New scroll offset that keeps `target` inside `offset..offset + extent`.
fn scroll_axis(offset: usize, target: usize, extent: usize) -> usize {
    if extent == 0 || target < offset {
        return target.min(offset);
    }
    if target >= offset + extent {
        return target + 1 - extent;
    }
    offset
}
