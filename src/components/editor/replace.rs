//! Replacing several stretches of text at once, as one undo step.

use std::ops::Range;

use crate::buffer::{Buffer, BufferError, Edit, Position, Selection, Selections};

use super::Editor;

/// One stretch of text to replace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Replacement {
    pub(crate) start: Position,
    pub(crate) end: Position,
    pub(crate) text: String,
}

impl Editor {
    /// Replaces each of `replacements`, which must be in text order and not
    /// overlap, as one undo step, and puts the cursor at `cursor` (a position of
    /// the text as it is after the replacing).
    pub(crate) fn replace_all_of(
        &mut self,
        replacements: &[Replacement],
        cursor: Position,
    ) -> Result<(), BufferError> {
        let mut edits = Vec::with_capacity(replacements.len());
        for replacement in replacements {
            let start = self.buffer.position_to_byte(replacement.start)?;
            let end = self.buffer.position_to_byte(replacement.end)?;
            edits.push(Edit::new(start..end, replacement.text.as_str()));
        }
        self.commit_replacements(edits, |_| Ok(Selection::cursor(cursor)))
    }

    /// Replaces each byte range of `replacements` (in text order, not
    /// overlapping, text as it is now) as one undo step, and puts the cursor at
    /// the bytes `anchor..cursor` of the text as it is after the replacing selected
    /// (a plain cursor when they are the same).
    pub(crate) fn replace_ranges(
        &mut self,
        replacements: &[(Range<usize>, &str)],
        anchor: usize,
        cursor: usize,
    ) -> Result<(), BufferError> {
        let edits = replacements
            .iter()
            .map(|(range, text)| Edit::new(range.clone(), *text))
            .collect();
        self.commit_replacements(edits, |buffer| {
            Ok(Selection::new(
                buffer.byte_to_position(anchor)?,
                buffer.byte_to_position(cursor)?,
            ))
        })
    }

    fn commit_replacements(
        &mut self,
        edits: Vec<Edit>,
        selection: impl FnOnce(&Buffer) -> Result<Selection, BufferError>,
    ) -> Result<(), BufferError> {
        if edits.is_empty() {
            return Ok(());
        }

        self.view.last_edit = None;
        let before = self.view.selections.clone();
        let mut transaction = self.buffer.begin_transaction(before);
        for edit in edits.iter().rev() {
            transaction.apply(edit)?;
        }
        let after = Selections::single(selection(transaction.buffer())?);
        transaction.commit(after.clone());
        self.view.selections = after;
        Ok(())
    }
}
