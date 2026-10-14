//! Editing with more than one cursor: one change per selection, applied as one
//! undo step. Only these operations (typing, a line break, the deletes, the
//! motions and the extension of selections) know about several cursors; the
//! others act on the primary one and so go back to a single cursor.

use std::ops::Range;

use crate::buffer::{BufferError, Edit, Position, Selection, Selections};

use super::{EditKind, Editor, Motion};

/// What one cursor changes: the byte range it replaces and the text it puts
/// there. `None` for a cursor with nothing to do.
type Planned = Option<(Range<usize>, String)>;

/// Which delete is meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Delete {
    Backward,
    Forward,
    WordBackward,
    WordForward,
}

impl Editor {
    /// Whether more than one cursor is active.
    pub(crate) fn has_many_cursors(&self) -> bool {
        !self.view.selections.is_single()
    }

    /// Back to the primary cursor alone.
    pub(crate) fn collapse_cursors(&mut self) {
        self.view.selections = self.view.selections.collapsed();
        self.view.last_edit = None;
    }

    /// Adds a cursor on the line `lines` below the primary one (above for a
    /// negative number), in the column that cursor is aiming for. The new cursor
    /// is the primary one, so pressing again keeps going. Does nothing at the
    /// first or last line.
    pub(crate) fn add_cursor_vertically(&mut self, lines: isize) -> Result<(), BufferError> {
        self.view.last_edit = None;
        let primary = *self.view.selections.primary();
        let head = primary.head();
        let Some(line) = head.line.checked_add_signed(lines) else {
            return Ok(());
        };
        if line >= self.buffer.len_lines() {
            return Ok(());
        }
        let wanted = primary.desired_column().unwrap_or(head.column);
        let column = wanted.min(self.buffer.line_len(line)?);
        let added =
            Selection::cursor(Position::new(line, column)).with_desired_column(Some(wanted));
        self.view.selections = self.view.selections.with_added(added);
        Ok(())
    }

    #[cfg(test)]
    /// Adds a cursor at `position`, or the nearest place that exists, and makes
    /// it the primary one.
    pub(crate) fn add_cursor(&mut self, position: Position) {
        self.view.last_edit = None;
        let position = self.buffer.clamp_position(position);
        let added = Selection::cursor(position);
        self.view.selections = self.view.selections.with_added(added);
    }

    /// Replaces every selection (or inserts at every cursor) with `text`.
    pub(super) fn insert_at_every_cursor(&mut self, text: &str) -> Result<(), BufferError> {
        let continues_typing = !text.contains(['\n', '\r']);
        self.edit_each(
            continues_typing.then_some(EditKind::Typing),
            |editor, selection| {
                let range = editor.bytes_of(selection)?;
                Ok((!(range.is_empty() && text.is_empty())).then(|| (range, text.to_owned())))
            },
        )
    }

    /// A line break at every cursor, each followed by the indentation of its
    /// line as far as it lies left of the cursor.
    pub(super) fn newline_at_every_cursor(&mut self) -> Result<(), BufferError> {
        let line_break = self.buffer.line_ending().as_str();
        self.edit_each(None, |editor, selection| {
            let start = selection.start();
            let content = editor.buffer.line_content(start.line)?;
            let indent: String = content
                .chars()
                .take_while(|c| matches!(c, ' ' | '\t'))
                .take(start.column)
                .collect();
            Ok(Some((
                editor.bytes_of(selection)?,
                format!("{line_break}{indent}"),
            )))
        })
    }

    /// One delete at every cursor; a selection is deleted whole.
    pub(super) fn delete_at_every_cursor(&mut self, delete: Delete) -> Result<(), BufferError> {
        let kind = match delete {
            Delete::Backward | Delete::WordBackward => EditKind::Backspace,
            Delete::Forward | Delete::WordForward => EditKind::Delete,
        };
        self.edit_each(Some(kind), |editor, selection| {
            let range = if selection.is_empty() {
                editor.delete_range(selection, delete)?
            } else {
                editor.bytes_of(selection)?
            };
            Ok((!range.is_empty()).then(|| (range, String::new())))
        })
    }

    fn bytes_of(&self, selection: &Selection) -> Result<Range<usize>, BufferError> {
        let start = self.buffer.position_to_byte(selection.start())?;
        let end = self.buffer.position_to_byte(selection.end())?;
        Ok(start..end)
    }

    /// The bytes a delete removes next to a cursor.
    fn delete_range(
        &self,
        cursor: &Selection,
        delete: Delete,
    ) -> Result<Range<usize>, BufferError> {
        let head = cursor.head();
        let (from, to) = match delete {
            Delete::Backward => {
                let start = if head.column > 0 {
                    Position::new(head.line, head.column - 1)
                } else if let Some(line) = head.line.checked_sub(1) {
                    Position::new(line, self.buffer.line_len(line)?)
                } else {
                    head
                };
                (start, head)
            }
            Delete::Forward => {
                let end = if head.column < self.buffer.line_len(head.line)? {
                    Position::new(head.line, head.column + 1)
                } else if head.line + 1 < self.buffer.len_lines() {
                    Position::new(head.line + 1, 0)
                } else {
                    head
                };
                (head, end)
            }
            Delete::WordBackward => (self.target(cursor, Motion::WordLeft)?.position, head),
            Delete::WordForward => (head, self.target(cursor, Motion::WordRight)?.position),
        };
        Ok(self.buffer.position_to_byte(from)?..self.buffer.position_to_byte(to)?)
    }

    /// Applies what `plan` says for each selection, all against the text as it
    /// is now, as one undo step, and leaves a cursor after each change.
    fn edit_each(
        &mut self,
        kind: Option<EditKind>,
        plan: impl Fn(&Self, &Selection) -> Result<Planned, BufferError>,
    ) -> Result<(), BufferError> {
        let selections: Vec<Selection> = self.view.selections.iter().copied().collect();
        let mut planned = Vec::with_capacity(selections.len());
        for selection in &selections {
            planned.push(plan(self, selection)?);
        }
        let changes = join_overlapping(planned.iter().flatten().cloned().collect());
        if changes.is_empty() {
            return Ok(());
        }

        let previous = self.view.last_edit.take();
        let mut heads = Vec::with_capacity(selections.len());
        for selection in &selections {
            heads.push(self.buffer.position_to_byte(selection.head())?);
        }
        let mut transaction = self.buffer.begin_transaction(self.view.selections.clone());
        // Last first, so the offsets of the changes before it stay right.
        for (range, text) in changes.iter().rev() {
            transaction.apply(&Edit::new(range.clone(), text.as_str()))?;
        }

        let mut cursors = Vec::with_capacity(selections.len());
        for ((plan, head), _) in planned.iter().zip(&heads).zip(&selections) {
            let byte = match plan {
                Some((range, _)) => after_change(&changes, range),
                None => moved_by(&changes, *head),
            };
            cursors.push(Selection::cursor(
                transaction.buffer().byte_to_position(byte)?,
            ));
        }
        let after = Selections::from_items(cursors, self.view.selections.primary_index());
        transaction.commit(after.clone());
        self.view.selections = after;

        if kind.is_some() && previous == kind {
            self.buffer.merge_last_two_steps();
        }
        self.view.last_edit = kind;
        Ok(())
    }
}

/// How much longer the text gets by `change`.
fn growth((range, text): &(Range<usize>, String)) -> isize {
    text.len() as isize - range.len() as isize
}

/// Where the text of the change that covers `wanted` ends, in the text after all
/// `changes` (in order, not overlapping) are applied: that is where its cursor goes.
fn after_change(changes: &[(Range<usize>, String)], wanted: &Range<usize>) -> usize {
    let at = changes
        .iter()
        .position(|(range, _)| range == wanted)
        .or_else(|| {
            changes
                .iter()
                .position(|(range, _)| range.start <= wanted.start && wanted.end <= range.end)
        })
        .unwrap_or(0);
    let before: isize = changes[..at].iter().map(growth).sum();
    let (range, text) = &changes[at];
    (range.start as isize + before).max(0) as usize + text.len()
}

/// Where `byte` of the old text is after `changes`: moved by each change that ends
/// at or before it, or at the end of the new text when a change covers it.
fn moved_by(changes: &[(Range<usize>, String)], byte: usize) -> usize {
    let mut moved = 0isize;
    for change in changes {
        let (range, text) = change;
        if range.end <= byte {
            moved += growth(change);
        } else if range.start < byte {
            return (range.start as isize + moved).max(0) as usize + text.len();
        } else {
            break;
        }
    }
    (byte as isize + moved).max(0) as usize
}

/// The changes in order, those that run into one another joined into one that
/// covers both (this only happens with deletes, whose text is empty).
fn join_overlapping(mut changes: Vec<(Range<usize>, String)>) -> Vec<(Range<usize>, String)> {
    changes.sort_by_key(|(range, _)| (range.start, range.end));
    let mut joined: Vec<(Range<usize>, String)> = Vec::with_capacity(changes.len());
    for (range, text) in changes {
        match joined.last_mut() {
            Some((last, last_text)) if range.start < last.end => {
                last.end = last.end.max(range.end);
                last_text.push_str(&text);
            }
            _ => joined.push((range, text)),
        }
    }
    joined
}
