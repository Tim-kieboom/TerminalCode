//! Replacing several stretches of text at once, as one undo step.

use crate::buffer::{BufferError, Edit, Position, Selection, Selections};

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
        if edits.is_empty() {
            return Ok(());
        }

        self.view.last_edit = None;
        let before = self.view.selections.clone();
        let mut transaction = self.buffer.begin_transaction(before);
        for edit in edits.iter().rev() {
            transaction.apply(edit)?;
        }
        let after = Selections::single(Selection::cursor(cursor));
        transaction.commit(after.clone());
        self.view.selections = after;
        Ok(())
    }
}
