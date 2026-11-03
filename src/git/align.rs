//! Two texts side by side: which row of the screen shows which line of each. A changed
//! stretch takes as many rows as the longer of its two sides has, and the shorter side
//! gets filler rows (no line) for the rest, so what is the same stands in the same row.

use std::ops::Range;

use super::diff::{Block, blocks};

/// Which of the two texts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    Old,
    New,
}

/// A changed stretch and the row it starts at.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Placed {
    old: Range<usize>,
    new: Range<usize>,
    row: usize,
}

impl Placed {
    fn lines(&self, side: Side) -> &Range<usize> {
        match side {
            Side::Old => &self.old,
            Side::New => &self.new,
        }
    }

    /// The rows it takes: those of the longer side.
    fn height(&self) -> usize {
        self.old.len().max(self.new.len())
    }
}

/// How two texts line up, from the hash of each of their lines.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Alignment {
    placed: Vec<Placed>,
    old_len: usize,
    new_len: usize,
    rows: usize,
}

impl Alignment {
    pub(crate) fn new(old: &[u64], new: &[u64]) -> Self {
        let mut placed = Vec::new();
        // The rows above the next stretch: what came before it, line for line.
        let (mut old_end, mut new_end, mut row) = (0, 0, 0);
        for Block { old, new } in blocks(old, new) {
            row += old.start - old_end;
            let stretch = Placed { old, new, row };
            row += stretch.height();
            (old_end, new_end) = (stretch.old.end, stretch.new.end);
            placed.push(stretch);
        }
        // What is left after the last stretch is the same in both texts.
        let rows = row + new.len().saturating_sub(new_end);
        Self {
            placed,
            old_len: old.len(),
            new_len: new.len(),
            rows,
        }
    }

    /// How many rows the two texts take together.
    pub(crate) fn rows(&self) -> usize {
        self.rows
    }

    /// Whether the texts are the same.
    pub(crate) fn is_same(&self) -> bool {
        self.placed.is_empty()
    }

    fn len(&self, side: Side) -> usize {
        match side {
            Side::Old => self.old_len,
            Side::New => self.new_len,
        }
    }

    /// The row `line` of `side` is in. A line past the end goes on in the rows after the
    /// last one.
    pub(crate) fn row_of(&self, side: Side, line: usize) -> usize {
        let before = self
            .placed
            .partition_point(|stretch| stretch.lines(side).end <= line);
        if let Some(stretch) = self.placed.get(before) {
            let lines = stretch.lines(side);
            if lines.start <= line {
                return stretch.row + (line - lines.start);
            }
        }
        match before.checked_sub(1).and_then(|at| self.placed.get(at)) {
            Some(last) => last.row + last.height() + (line - last.lines(side).end),
            None => line,
        }
    }

    /// The line of `side` that is in `row`; `None` for a filler row, or a row past the
    /// text.
    pub(crate) fn line_at(&self, side: Side, row: usize) -> Option<usize> {
        let up_to = self.placed.partition_point(|stretch| stretch.row <= row);
        let line = match up_to.checked_sub(1).and_then(|at| self.placed.get(at)) {
            None => row,
            Some(stretch) => {
                let offset = row - stretch.row;
                let lines = stretch.lines(side);
                if offset < stretch.height() {
                    return (offset < lines.len()).then_some(lines.start + offset);
                }
                lines.end + (offset - stretch.height())
            }
        };
        (line < self.len(side)).then_some(line)
    }

    /// The line of `side` to use for `row` when a line is needed: the one in it, or for a
    /// filler row the nearest line above it (the first line when none is).
    pub(crate) fn nearest_line(&self, side: Side, row: usize) -> usize {
        let row = row.min(self.rows.saturating_sub(1));
        (0..=row)
            .rev()
            .find_map(|at| self.line_at(side, at))
            .unwrap_or(0)
    }

    /// Whether `row` is in a changed stretch.
    pub(crate) fn is_changed(&self, row: usize) -> bool {
        let up_to = self.placed.partition_point(|stretch| stretch.row <= row);
        up_to
            .checked_sub(1)
            .and_then(|at| self.placed.get(at))
            .is_some_and(|stretch| row < stretch.row + stretch.height())
    }

    /// The line of the new text the first change after `line` is at: where a removal
    /// is, it is the line that follows what was removed.
    pub(crate) fn next_change(&self, line: usize) -> Option<usize> {
        let found = self
            .placed
            .iter()
            .find(|stretch| stretch.new.start > line)?;
        Some(self.anchor(found))
    }

    /// The line the last change before `line` starts at.
    pub(crate) fn previous_change(&self, line: usize) -> Option<usize> {
        let found = self
            .placed
            .iter()
            .rev()
            .find(|stretch| stretch.new.start < line)?;
        Some(self.anchor(found))
    }

    /// The line of the new text a change is shown at, held to the text.
    fn anchor(&self, stretch: &Placed) -> usize {
        stretch.new.start.min(self.new_len.saturating_sub(1))
    }
}

#[cfg(test)]
mod tests;
