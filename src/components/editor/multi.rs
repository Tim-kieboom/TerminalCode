//! Editing with more than one cursor: one change per selection, applied as one
//! undo step. Only these operations (typing, a line break, the deletes, the
//! motions and the extension of selections) know about several cursors; the
//! others act on the primary one and so go back to a single cursor.

use std::cell::Cell;
use std::ops::Range;

use crate::buffer::{BufferError, Edit, Position, Selection, Selections};
use crate::clipboard::{Register, RegisterKind};

use super::{EditKind, Editor, Motion};

/// What one cursor changes: the byte range it replaces and the text it puts
/// there. `None` for a cursor with nothing to do.
type Planned = Option<(Range<usize>, String)>;

/// A [`Planned`] with the bytes of its text that stay after the cursor.
type PlannedLeaving = Option<(Range<usize>, String, usize)>;

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
                let mut range = editor.bytes_of(selection)?;
                // A closing bracket on a line of only indentation takes a level of it away.
                if let Some(dedent) = editor.closer_dedent_for(selection, text)? {
                    range = dedent.start..range.end;
                }
                Ok((!(range.is_empty() && text.is_empty())).then(|| (range, text.to_owned())))
            },
        )
    }

    /// A line break at every cursor, as `Enter` makes it with one: each followed by the
    /// indentation of its line as far as it lies left of the cursor, one level deeper after
    /// an opening bracket, and a closing bracket right after the cursor sent to a line of its own.
    pub(super) fn newline_at_every_cursor(&mut self) -> Result<(), BufferError> {
        self.edit_each_leaving(None, false, |editor, selection| {
            let (text, leaving) = editor.newline_text(selection)?;
            Ok(Some((editor.bytes_of(selection)?, text, leaving)))
        })
    }

    /// Tab with several cursors: when a selection covers several lines, those lines (and the
    /// lines of the other cursors) move one level in; otherwise every cursor gets one level of
    /// indentation (to the next tab stop for spaces) in place of what it has selected.
    pub(super) fn indent_at_every_cursor(&mut self) -> Result<(), BufferError> {
        if self
            .view
            .selections
            .iter()
            .any(|selection| selection.start().line != selection.end().line)
        {
            return self.indent_line();
        }
        let all_bare = self.view.selections.iter().all(Selection::is_empty);
        let style = self.view.indent;
        self.edit_each(
            all_bare.then_some(EditKind::Typing),
            move |editor, selection| {
                let text = style.tab_text(selection.start().column);
                Ok(Some((editor.bytes_of(selection)?, text)))
            },
        )
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

    /// The selections joined by line breaks, a bare cursor giving its line (once
    /// when cursors share it). Linewise only when no cursor has a selection.
    pub(super) fn copy_every_cursor(&self) -> Result<Register, BufferError> {
        let ending = self.buffer.line_ending().as_str();
        let mut pieces: Vec<String> = Vec::new();
        let mut any_selection = false;
        let mut last_line = None;
        for selection in self.view.selections.iter() {
            if selection.is_empty() {
                let line = selection.head().line;
                if last_line.replace(line) == Some(line) {
                    continue;
                }
                pieces.push(self.buffer.line_content(line)?.into_owned());
            } else {
                any_selection = true;
                pieces.push(self.buffer.text_in(self.bytes_of(selection)?)?);
            }
        }
        let mut text = pieces.join(ending);
        if any_selection {
            return Ok(Register::charwise(text));
        }
        text.push_str(ending);
        Ok(Register::linewise(text))
    }

    /// Copies as above, then deletes every selection and every bare cursor's line.
    pub(super) fn cut_every_cursor(&mut self) -> Result<Register, BufferError> {
        let register = self.copy_every_cursor()?;
        self.edit_each(None, |editor, selection| {
            let range = if selection.is_empty() {
                editor.line_range(selection.head().line)?
            } else {
                editor.bytes_of(selection)?
            };
            Ok((!range.is_empty()).then(|| (range, String::new())))
        })?;
        Ok(register)
    }

    /// Pastes `text` (its line breaks already the buffer's) at every cursor. When it
    /// has as many lines as there are cursors, cursor number n gets line n;
    /// otherwise every cursor gets all of it. Linewise text with no selection
    /// anywhere goes above the cursor's line (above each distinct line when every
    /// cursor gets all of it), the cursors staying where they were; otherwise it
    /// replaces each selection or goes in at each cursor, as plain text.
    pub(super) fn paste_at_every_cursor(
        &mut self,
        kind: RegisterKind,
        text: &str,
    ) -> Result<(), BufferError> {
        let ending = self.buffer.line_ending().as_str();
        let linewise = kind == RegisterKind::Linewise;
        let text = if linewise {
            text.strip_suffix(ending).unwrap_or(text)
        } else {
            text
        };
        let parts: Vec<&str> = text.split(ending).collect();
        let split = parts.len() == self.view.selections.len();
        let next = Cell::new(0usize);
        let piece = |next: &Cell<usize>| -> &str {
            let at = next.replace(next.get() + 1);
            if split { parts[at] } else { text }
        };

        if linewise && self.view.selections.iter().all(Selection::is_empty) {
            let last_line = Cell::new(None);
            return self.edit_each_placing(None, true, |editor, selection| {
                let mut lines = piece(&next).to_owned();
                lines.push_str(ending);
                let line = selection.head().line;
                if last_line.replace(Some(line)) == Some(line) && !split {
                    return Ok(None);
                }
                let start = editor.buffer.position_to_byte(Position::new(line, 0))?;
                Ok(Some((start..start, lines)))
            });
        }
        self.edit_each(None, |editor, selection| {
            let text = piece(&next);
            let range = editor.bytes_of(selection)?;
            Ok((!(range.is_empty() && text.is_empty())).then(|| (range, text.to_owned())))
        })
    }

    pub(super) fn bytes_of(&self, selection: &Selection) -> Result<Range<usize>, BufferError> {
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
        self.edit_each_placing(kind, false, plan)
    }

    /// `edit_each`; with `stay` every cursor keeps its place in the text instead of
    /// going after its change.
    fn edit_each_placing(
        &mut self,
        kind: Option<EditKind>,
        stay: bool,
        plan: impl Fn(&Self, &Selection) -> Result<Planned, BufferError>,
    ) -> Result<(), BufferError> {
        self.edit_each_leaving(kind, stay, |editor, selection| {
            Ok(plan(editor, selection)?.map(|(range, text)| (range, text, 0)))
        })
    }

    /// `edit_each_placing`, where a change may also say how many bytes of its text stay
    /// after its cursor (a line break that opens a line between two brackets puts the
    /// cursor on the empty line, with the closing bracket's line after it).
    fn edit_each_leaving(
        &mut self,
        kind: Option<EditKind>,
        stay: bool,
        plan: impl Fn(&Self, &Selection) -> Result<PlannedLeaving, BufferError>,
    ) -> Result<(), BufferError> {
        let selections: Vec<Selection> = self.view.selections.iter().copied().collect();
        let mut planned = Vec::with_capacity(selections.len());
        for selection in &selections {
            planned.push(plan(self, selection)?);
        }
        let changes = join_overlapping(
            planned
                .iter()
                .flatten()
                .map(|(range, text, _)| (range.clone(), text.clone()))
                .collect(),
        );
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
                Some((range, _, leaving)) if !stay => {
                    after_change(&changes, range).saturating_sub(*leaving)
                }
                _ => moved_by(&changes, *head),
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
