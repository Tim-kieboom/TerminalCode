use super::Buffer;
use super::edit::Edit;
use super::error::BufferError;
use super::selection::Selections;

/// One undoable change: everything applied between `begin_transaction` and
/// `commit`, plus the selections to restore on either side.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Record {
    edits: Vec<Edit>,
    inverses: Vec<Edit>,
    before: Selections,
    after: Selections,
}

/// Linear undo/redo stacks. A new change clears the redo stack.
#[derive(Debug, Default)]
pub(super) struct History {
    undo: Vec<Record>,
    redo: Vec<Record>,
}

/// A group of edits that undo and redo as one step. Obtained from
/// [`Buffer::begin_transaction`].
///
/// Dropping it without calling [`Transaction::commit`] rolls the edits back.
#[derive(Debug)]
pub(crate) struct Transaction<'a> {
    buffer: &'a mut Buffer,
    record: Option<Record>,
}

impl Transaction<'_> {
    pub(crate) fn apply(&mut self, edit: &Edit) -> Result<(), BufferError> {
        let applied = self.buffer.apply(edit)?;
        if let Some(record) = &mut self.record {
            record.edits.push(edit.clone());
            record.inverses.push(applied.inverse);
        }
        Ok(())
    }

    /// Ends the transaction as one undo step. A transaction without edits
    /// leaves no step behind.
    pub(crate) fn commit(mut self, after: Selections) {
        let Some(mut record) = self.record.take() else {
            return;
        };
        if record.edits.is_empty() {
            return;
        }
        record.after = after;
        self.buffer.history.undo.push(record);
        self.buffer.history.redo.clear();
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        let Some(record) = self.record.take() else {
            return;
        };
        // Inverses are exact, so this cannot fail unless the buffer was
        // changed behind the transaction's back, which `&mut` rules out.
        let rolled_back = revert(self.buffer, &record.inverses);
        debug_assert!(rolled_back.is_ok(), "rollback failed: {rolled_back:?}");
    }
}

impl Buffer {
    pub(crate) fn begin_transaction(&mut self, before: Selections) -> Transaction<'_> {
        Transaction {
            buffer: self,
            record: Some(Record {
                edits: Vec::new(),
                inverses: Vec::new(),
                after: before.clone(),
                before,
            }),
        }
    }

    /// Reverts the latest step. Returns the selections from before it, or
    /// `None` when there is nothing to undo.
    pub(crate) fn undo(&mut self) -> Result<Option<Selections>, BufferError> {
        let Some(record) = self.history.undo.pop() else {
            return Ok(None);
        };
        revert(self, &record.inverses)?;
        let before = record.before.clone();
        self.history.redo.push(record);
        Ok(Some(before))
    }

    /// Re-applies the latest undone step. Returns the selections from after
    /// it, or `None` when there is nothing to redo.
    pub(crate) fn redo(&mut self) -> Result<Option<Selections>, BufferError> {
        let Some(record) = self.history.redo.pop() else {
            return Ok(None);
        };
        for edit in &record.edits {
            self.apply(edit)?;
        }
        let after = record.after.clone();
        self.history.undo.push(record);
        Ok(Some(after))
    }
}

/// Applies `inverses` newest first.
fn revert(buffer: &mut Buffer, inverses: &[Edit]) -> Result<(), BufferError> {
    for inverse in inverses.iter().rev() {
        buffer.apply(inverse)?;
    }
    Ok(())
}
