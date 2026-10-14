use std::ops::Range;

#[cfg(test)]
use ratatui::layout::Rect;

use crate::buffer::{Buffer, BufferError, Edit, Position, Selection, Selections};

pub(crate) use indent::{IndentKind, IndentPrefs, IndentStyle};
pub(crate) use motion::Motion;
pub(crate) use replace::Replacement;
pub(crate) use text_layout::TabWidth;
pub(crate) use view_state::ViewState;

mod clipboard_ops;
mod indent;
mod motion;
mod mouse_ops;
mod multi;
pub(crate) mod render;
mod replace;
mod smart_indent;
#[cfg(test)]
mod tests;
mod text_layout;
mod view_state;

/// File name for titles and the status bar, with `[+]` when modified.
pub(crate) fn display_name(buffer: &Buffer) -> String {
    let name = buffer
        .path()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy())
        .or_else(|| buffer.title().map(std::borrow::Cow::Borrowed))
        .unwrap_or_else(|| "[no name]".into());

    match buffer.is_dirty() {
        true => format!("{name} [+]"),
        false => name.into_owned(),
    }
}

/// Read-only access to one editor: a document and a view of it. What anything
/// that only looks at the active editor gets.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EditorRef<'a> {
    buffer: &'a Buffer,
    view: &'a ViewState,
}

impl<'a> EditorRef<'a> {
    pub(crate) fn new(buffer: &'a Buffer, view: &'a ViewState) -> Self {
        Self { buffer, view }
    }

    pub(crate) fn buffer(&self) -> &'a Buffer {
        self.buffer
    }

    pub(crate) fn view(&self) -> &'a ViewState {
        self.view
    }

    pub(crate) fn selections(&self) -> &'a Selections {
        self.view.selections()
    }

    /// The screen cell of the cursor, as of the last layout pass; `None` when it
    /// is scrolled out of view or the editor has not been drawn.
    pub(crate) fn cursor_cell(&self) -> Option<ratatui::layout::Position> {
        let area = self.view.text_area()?;
        let head = self.view.selections().primary().head();
        let content = self.buffer.line_content(head.line).ok()?;
        let column = text_layout::display_column(&content, head.column, self.view.tab_width());
        let scroll = self.view.scroll();
        let row = head.line.checked_sub(scroll.top)?;
        let column = column.checked_sub(scroll.left)?;
        if row >= usize::from(area.height) || column >= usize::from(area.width) {
            return None;
        }
        Some(ratatui::layout::Position::new(
            area.x + column as u16,
            area.y + row as u16,
        ))
    }

    /// The byte of the character under a screen cell, as of the last layout pass;
    /// `None` when the cell is outside the text, past the end of its line, or on
    /// a line the text does not have.
    pub(crate) fn byte_at_cell(&self, column: u16, row: u16) -> Option<usize> {
        let area = self.view.text_area()?;
        if !area.contains(ratatui::layout::Position::new(column, row)) {
            return None;
        }
        let scroll = self.view.scroll();
        let line = scroll.top + usize::from(row - area.y);
        let content = self.buffer.line_content(line).ok()?;
        let display = scroll.left + usize::from(column - area.x);
        let tab_width = self.view.tab_width();
        if display >= text_layout::display_column(&content, usize::MAX, tab_width) {
            return None;
        }
        let at = text_layout::column_at_display(&content, display, tab_width);
        let position = crate::buffer::Position::new(line, at);
        self.buffer.position_to_byte(position).ok()
    }

    #[cfg(test)]
    pub(crate) fn scroll(&self) -> Scroll {
        self.view.scroll()
    }

    #[cfg(test)]
    pub(crate) fn text_area(&self) -> Option<Rect> {
        self.view.text_area()
    }

    pub(crate) fn display_name(&self) -> String {
        display_name(self.buffer)
    }
}

/// First visible line and display column of the editor viewport.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Scroll {
    pub(crate) top: usize,
    pub(crate) left: usize,
}

/// What kind of edit the previous editing action was. Consecutive edits of
/// the same kind form one undo step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditKind {
    Typing,
    Backspace,
    Delete,
}

/// One open document with its selection and viewport.
///
/// There can be several cursors (see `multi.rs`): typing, line breaks, deletes,
/// motions and selecting by motion act on each of them; everything else acts on
/// the primary one and leaves a single cursor.
#[derive(Debug, Default)]
pub(crate) struct Editor {
    buffer: Buffer,
    view: ViewState,
}

impl Editor {
    pub(crate) fn new(buffer: Buffer) -> Self {
        let indent = IndentStyle::detect(&buffer).unwrap_or_default();
        Self {
            buffer,
            view: ViewState::with_indent(indent),
        }
    }

    /// Joins a view with the document text it shows.
    pub(crate) fn from_parts(view: ViewState, buffer: Buffer) -> Self {
        Self { buffer, view }
    }

    pub(crate) fn into_parts(self) -> (ViewState, Buffer) {
        (self.view, self.buffer)
    }

    #[cfg(test)]
    pub(crate) fn view(&self) -> &ViewState {
        &self.view
    }

    pub(crate) fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    #[cfg(test)]
    pub(crate) fn selections(&self) -> &Selections {
        &self.view.selections
    }

    #[cfg(test)]
    pub(crate) fn scroll(&self) -> Scroll {
        self.view.scroll
    }

    /// The edits applied to the buffer since the last call; see
    /// [`Buffer::take_edit_log`].
    pub(crate) fn take_edit_log(&mut self) -> Vec<crate::buffer::EditInfo> {
        self.buffer.take_edit_log()
    }

    /// Moves every cursor. With a selection, left and right collapse it to its
    /// start and end instead of moving.
    pub(crate) fn move_cursor(&mut self, motion: Motion) -> Result<(), BufferError> {
        self.view.last_edit = None;
        let primary = *self.view.selections.primary();
        let mut moved = Vec::with_capacity(self.view.selections.len());
        for selection in self.view.selections.iter() {
            moved.push(self.moved(selection, motion)?);
        }
        let target = self.moved(&primary, motion)?;
        self.follow_page_motion(motion, primary.head().line, target.head().line);
        self.view.selections = Selections::from_items(moved, self.view.selections.primary_index());
        Ok(())
    }

    fn moved(&self, selection: &Selection, motion: Motion) -> Result<Selection, BufferError> {
        let collapse_to = match motion {
            Motion::Left if !selection.is_empty() => Some(selection.start()),
            Motion::Right if !selection.is_empty() => Some(selection.end()),
            _ => None,
        };
        Ok(match collapse_to {
            Some(position) => Selection::cursor(position),
            None => {
                let target = self.target(selection, motion)?;
                Selection::cursor(target.position).with_desired_column(target.desired_column)
            }
        })
    }

    /// Moves the head of every selection, keeping the anchors.
    pub(crate) fn select(&mut self, motion: Motion) -> Result<(), BufferError> {
        self.view.last_edit = None;
        let primary = *self.view.selections.primary();
        let mut extended = Vec::with_capacity(self.view.selections.len());
        for selection in self.view.selections.iter() {
            let target = self.target(selection, motion)?;
            extended.push(
                Selection::new(selection.anchor(), target.position)
                    .with_desired_column(target.desired_column),
            );
        }
        let target = self.target(&primary, motion)?;
        self.follow_page_motion(motion, primary.head().line, target.position.line);
        self.view.selections =
            Selections::from_items(extended, self.view.selections.primary_index());
        Ok(())
    }

    /// Puts the cursor at `position`, or the nearest place that exists.
    pub(crate) fn go_to(&mut self, position: Position) {
        self.view.last_edit = None;
        let position = self.buffer.clamp_position(position);
        self.view.selections = Selections::single(Selection::cursor(position));
    }

    /// Selects from `anchor` to `head`, or the nearest places that exist.
    pub(crate) fn select_range(&mut self, anchor: Position, head: Position) {
        self.view.last_edit = None;
        let anchor = self.buffer.clamp_position(anchor);
        let head = self.buffer.clamp_position(head);
        self.view.selections = Selections::single(Selection::new(anchor, head));
    }

    /// Selects the whole document.
    pub(crate) fn select_all(&mut self) -> Result<(), BufferError> {
        self.view.last_edit = None;
        let last_line = self.buffer.len_lines() - 1;
        let end = Position::new(last_line, self.buffer.line_len(last_line)?);

        self.view.selections = Selections::single(Selection::new(Position::default(), end));
        Ok(())
    }

    /// Replaces the selection (or inserts at the cursor) with `text`.
    pub(crate) fn insert_text(&mut self, text: &str) -> Result<(), BufferError> {
        if self.has_many_cursors() {
            return self.insert_at_every_cursor(text);
        }
        let range = self.selection_bytes()?;
        if range.is_empty() && text.is_empty() {
            return Ok(());
        }
        let range = match self.closer_dedent(text)? {
            Some(dedent) => dedent.start..range.end,
            None => range,
        };

        let continues_typing = range.is_empty() && !text.contains(['\n', '\r']);
        let kind = continues_typing.then_some(EditKind::Typing);
        self.replace(range, text, kind)
    }

    /// Inserts a line break in the file's own style.
    pub(crate) fn insert_newline(&mut self) -> Result<(), BufferError> {
        self.insert_newline_indented()
    }

    /// Deletes the selection, or the grapheme before the cursor. At the start
    /// of a line it joins the line with the previous one.
    pub(crate) fn delete_backward(&mut self) -> Result<(), BufferError> {
        if self.has_many_cursors() {
            return self.delete_at_every_cursor(multi::Delete::Backward);
        }
        let selection = *self.view.selections.primary();
        if !selection.is_empty() {
            return self.replace(self.selection_bytes()?, "", None);
        }

        let head = selection.head();
        let start = if head.column > 0 {
            Position::new(head.line, head.column - 1)
        } else if let Some(line) = head.line.checked_sub(1) {
            Position::new(line, self.buffer.line_len(line)?)
        } else {
            return Ok(());
        };
        self.replace_between(start, head, EditKind::Backspace)
    }

    /// Deletes the selection, or the grapheme after the cursor. At the end of
    /// a line it joins the next line onto it.
    pub(crate) fn delete_forward(&mut self) -> Result<(), BufferError> {
        if self.has_many_cursors() {
            return self.delete_at_every_cursor(multi::Delete::Forward);
        }
        let selection = *self.view.selections.primary();
        if !selection.is_empty() {
            return self.replace(self.selection_bytes()?, "", None);
        }

        let head = selection.head();
        let end = if head.column < self.buffer.line_len(head.line)? {
            Position::new(head.line, head.column + 1)
        } else if head.line + 1 < self.buffer.len_lines() {
            Position::new(head.line + 1, 0)
        } else {
            return Ok(());
        };
        self.replace_between(head, end, EditKind::Delete)
    }

    /// Deletes the selection, or back to the start of the previous word.
    pub(crate) fn delete_word_backward(&mut self) -> Result<(), BufferError> {
        if self.has_many_cursors() {
            return self.delete_at_every_cursor(multi::Delete::WordBackward);
        }
        let selection = *self.view.selections.primary();
        if !selection.is_empty() {
            return self.replace(self.selection_bytes()?, "", None);
        }

        let head = selection.head();
        let start = self.target(&selection, Motion::WordLeft)?.position;
        if start == head {
            return Ok(());
        }
        self.replace_between(start, head, EditKind::Backspace)
    }

    /// Deletes the selection, or forward to the end of the next word.
    pub(crate) fn delete_word_forward(&mut self) -> Result<(), BufferError> {
        if self.has_many_cursors() {
            return self.delete_at_every_cursor(multi::Delete::WordForward);
        }
        let selection = *self.view.selections.primary();
        if !selection.is_empty() {
            return self.replace(self.selection_bytes()?, "", None);
        }

        let head = selection.head();
        let end = self.target(&selection, Motion::WordRight)?.position;
        if end == head {
            return Ok(());
        }
        self.replace_between(head, end, EditKind::Delete)
    }

    pub(crate) fn undo(&mut self) -> Result<(), BufferError> {
        self.view.last_edit = None;
        if let Some(selections) = self.buffer.undo()? {
            self.view.selections = selections;
        }
        Ok(())
    }

    pub(crate) fn redo(&mut self) -> Result<(), BufferError> {
        self.view.last_edit = None;
        if let Some(selections) = self.buffer.redo()? {
            self.view.selections = selections;
        }
        Ok(())
    }

    /// Scrolls the minimum needed to bring `line` and `display_column` into a
    /// viewport of `height` by `width` cells.
    #[cfg(test)]
    pub(crate) fn scroll_to_show(
        &mut self,
        line: usize,
        display_column: usize,
        height: usize,
        width: usize,
    ) {
        self.view
            .scroll_to_show(line, display_column, height, width);
    }

    fn target(&self, selection: &Selection, motion: Motion) -> Result<motion::Target, BufferError> {
        motion::target(
            &self.buffer,
            selection.head(),
            selection.desired_column(),
            motion,
            self.page_lines(),
        )
    }

    /// Lines a page motion travels: a screenful minus one for context.
    fn page_lines(&self) -> usize {
        self.view.viewport_height.saturating_sub(1).max(1)
    }

    /// After a page motion the viewport moves by as many lines as the cursor
    /// did, so the cursor keeps its row on screen.
    fn follow_page_motion(&mut self, motion: Motion, from: usize, to: usize) {
        if !matches!(motion, Motion::PageUp | Motion::PageDown) {
            return;
        }
        let last_line = self.buffer.len_lines() - 1;
        let top = (self.view.scroll.top + to).saturating_sub(from);
        self.view.scroll.top = top.min(last_line);
    }

    fn selection_bytes(&self) -> Result<Range<usize>, BufferError> {
        let selection = self.view.selections.primary();
        let start = self.buffer.position_to_byte(selection.start())?;
        let end = self.buffer.position_to_byte(selection.end())?;
        Ok(start..end)
    }

    /// Deletes the text between two positions as part of a run of `kind`.
    fn replace_between(
        &mut self,
        start: Position,
        end: Position,
        kind: EditKind,
    ) -> Result<(), BufferError> {
        let range = self.buffer.position_to_byte(start)?..self.buffer.position_to_byte(end)?;
        self.replace(range, "", Some(kind))
    }

    /// One undoable edit that leaves the cursor just after `text`. It joins
    /// the previous undo step when both are the same `kind` of edit.
    fn replace(
        &mut self,
        range: Range<usize>,
        text: &str,
        kind: Option<EditKind>,
    ) -> Result<(), BufferError> {
        self.replace_with(range, text, kind, |buffer, end| {
            buffer.byte_to_position(end)
        })
    }

    /// Like [`Editor::replace`], but `cursor` picks where the cursor goes. It
    /// gets the edited buffer and the byte offset just after the new text.
    fn replace_with(
        &mut self,
        range: Range<usize>,
        text: &str,
        kind: Option<EditKind>,
        cursor: impl FnOnce(&Buffer, usize) -> Result<Position, BufferError>,
    ) -> Result<(), BufferError> {
        let previous = self.view.last_edit.take();
        let mut transaction = self.buffer.begin_transaction(self.view.selections.clone());
        transaction.apply(&Edit::new(range.clone(), text))?;
        let cursor = cursor(transaction.buffer(), range.start + text.len())?;

        let after = Selections::single(Selection::cursor(cursor));
        transaction.commit(after.clone());
        self.view.selections = after;

        if kind.is_some() && previous == kind {
            self.buffer.merge_last_two_steps();
        }
        self.view.last_edit = kind;
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
