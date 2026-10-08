use std::ops::Range;

#[cfg(test)]
use ratatui::layout::Rect;

use crate::buffer::{Buffer, BufferError, Edit, FileError, Position, Selection, Selections};

pub(crate) use indent::IndentStyle;
pub(crate) use motion::Motion;
pub(crate) use view_state::ViewState;

mod clipboard_ops;
mod indent;
mod motion;
mod mouse_ops;
pub(crate) mod render;
#[cfg(test)]
mod tests;
mod text_layout;
mod view_state;

/// File name for titles and the status bar, with `[+]` when modified.
pub(crate) fn display_name(buffer: &Buffer) -> String {
    let name = buffer
        .path()
        .and_then(|path| path.file_name())
        .map_or_else(|| "[no name]".into(), |name| name.to_string_lossy());

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

    #[cfg(test)]
    pub(crate) fn buffer(&self) -> &'a Buffer {
        self.buffer
    }

    #[cfg(test)]
    pub(crate) fn view(&self) -> &'a ViewState {
        self.view
    }

    pub(crate) fn selections(&self) -> &'a Selections {
        self.view.selections()
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
/// Only the primary selection is edited for now; multi-cursor editing comes
/// after 0.1.0.
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

    /// File name for titles and the status bar, with `[+]` when modified.
    #[cfg(test)]
    pub(crate) fn display_name(&self) -> String {
        display_name(&self.buffer)
    }

    /// The edits applied to the buffer since the last call; see
    /// [`Buffer::take_edit_log`].
    pub(crate) fn take_edit_log(&mut self) -> Vec<crate::buffer::EditInfo> {
        self.buffer.take_edit_log()
    }

    /// Moves the cursor. With a selection, left and right collapse it to its
    /// start and end instead of moving.
    pub(crate) fn move_cursor(&mut self, motion: Motion) -> Result<(), BufferError> {
        self.view.last_edit = None;
        let selection = *self.view.selections.primary();
        let collapse_to = match motion {
            Motion::Left if !selection.is_empty() => Some(selection.start()),
            Motion::Right if !selection.is_empty() => Some(selection.end()),
            _ => None,
        };

        let moved = match collapse_to {
            Some(position) => Selection::cursor(position),
            None => {
                let target = self.target(&selection, motion)?;
                self.follow_page_motion(motion, selection.head().line, target.position.line);
                Selection::cursor(target.position).with_desired_column(target.desired_column)
            }
        };
        self.view.selections = Selections::single(moved);
        Ok(())
    }

    /// Moves the head of the selection, keeping the anchor.
    pub(crate) fn select(&mut self, motion: Motion) -> Result<(), BufferError> {
        self.view.last_edit = None;
        let selection = *self.view.selections.primary();
        let target = self.target(&selection, motion)?;
        self.follow_page_motion(motion, selection.head().line, target.position.line);
        let extended = Selection::new(selection.anchor(), target.position)
            .with_desired_column(target.desired_column);

        self.view.selections = Selections::single(extended);
        Ok(())
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
        let range = self.selection_bytes()?;
        if range.is_empty() && text.is_empty() {
            return Ok(());
        }

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

    pub(crate) fn save(&mut self) -> Result<(), FileError> {
        self.buffer.save()
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
