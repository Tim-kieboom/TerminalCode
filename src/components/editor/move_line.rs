//! Move line: the line the cursor is on, or the lines a selection touches, swap
//! places with the line above or below.

use crate::buffer::{BufferError, Edit, Position, Selection, Selections};

use super::Editor;
use super::indent::{remap, selected_lines};

impl Editor {
    /// Moves the lines of the primary selection one line up (`-1`) or down
    /// (`1`), with the selection. Does nothing at the first or last line. With
    /// several cursors only the primary one moves and the others are dropped.
    pub(crate) fn move_lines(&mut self, direction: isize) -> Result<(), BufferError> {
        let selection = *self.view.selections.primary();
        let lines = selected_lines(&selection);
        let (first, last) = (*lines.start(), *lines.end());
        let Some(from) = first.checked_add_signed(direction.min(0)) else {
            return Ok(());
        };
        let to = last + usize::from(direction > 0);
        if to >= self.buffer.len_lines() {
            return Ok(());
        }

        // The block and its neighbour as they are now, and as they will be.
        let mut texts = Vec::with_capacity(to - from + 1);
        for line in from..=to {
            texts.push(self.buffer.line_content(line)?.into_owned());
        }
        let neighbour = if direction < 0 {
            texts.remove(0)
        } else {
            texts.pop().unwrap_or_default()
        };
        if direction < 0 {
            texts.push(neighbour);
        } else {
            texts.insert(0, neighbour);
        }

        let range = self.buffer.position_to_byte(Position::new(from, 0))?
            ..self
                .buffer
                .position_to_byte(Position::new(to, self.buffer.line_len(to)?))?;
        let moved = texts.join(self.buffer.line_ending().as_str());
        let after = remap(&selection, |position| {
            Position::new(
                position.line.saturating_add_signed(direction),
                position.column,
            )
        });
        self.edit_lines(vec![Edit::new(range, moved)], after)?;

        // A selection that ended at the start of the line after the block can end
        // past the text now.
        let moved = *self.view.selections.primary();
        let (anchor, head) = (
            self.buffer.clamp_position(moved.anchor()),
            self.buffer.clamp_position(moved.head()),
        );
        self.view.selections = Selections::single(Selection::new(anchor, head));
        Ok(())
    }
}
