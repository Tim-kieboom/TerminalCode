//! The bracket that matches the one next to the cursor: drawn in a style of its own
//! (theme slot `editor.bracket`) and the target of "Navigate: Go to Matching Bracket".
//!
//! A plain scan that counts the brackets of one kind, so a bracket inside a string or a
//! comment is counted like any other. The scan is bounded: a screenful of lines for the
//! highlight (it runs on every frame), [`JUMP_LINES`] for the jump, and it gives up on a
//! line longer than [`MAX_LINE_BYTES`] and after [`SCAN_BUDGET`] graphemes.

use unicode_segmentation::UnicodeSegmentation;

use crate::buffer::{Buffer, BufferError, Position, Selection, Selections};

use super::{EditKind, Editor, ViewState};

/// How many lines the jump looks through, each way.
pub(crate) const JUMP_LINES: usize = 10_000;

/// A line longer than this is not looked through (the cursor on it, or a line met on the way).
const MAX_LINE_BYTES: usize = 100_000;

/// The most graphemes one search looks at.
const SCAN_BUDGET: usize = 200_000;

const PAIRS: [(&str, &str); 3] = [("(", ")"), ("[", "]"), ("{", "}")];

/// A bracket next to the cursor and the one that matches it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Matched {
    /// The bracket next to the cursor.
    pub(crate) bracket: Position,
    /// The bracket that matches it.
    pub(crate) other: Position,
    /// Whether the bracket is before the cursor (so the cursor is just after it).
    pub(crate) cursor_after: bool,
}

/// The bracket at the cursor `head` (the one right after it, else the one right before it)
/// and its match, looking at most `max_lines` lines away.
pub(crate) fn find(buffer: &Buffer, head: Position, max_lines: usize) -> Option<Matched> {
    let content = buffer.line_content(head.line).ok()?;
    if content.len() > MAX_LINE_BYTES {
        return None;
    }
    let graphemes: Vec<&str> = content.graphemes(true).collect();
    let (column, cursor_after) = [(head.column, false), (head.column.wrapping_sub(1), true)]
        .into_iter()
        .find(|&(column, _)| {
            graphemes
                .get(column)
                .is_some_and(|g| PAIRS.iter().any(|&(o, c)| *g == o || *g == c))
        })?;
    let bracket = graphemes[column];
    let (open, close, forward) = PAIRS.iter().find_map(|&(open, close)| {
        (bracket == open)
            .then_some((open, close, true))
            .or_else(|| (bracket == close).then_some((open, close, false)))
    })?;
    let at = Position::new(head.line, column);
    let mut scanner = Scanner {
        buffer,
        open,
        close,
        depth: 1,
        budget: SCAN_BUDGET,
    };
    let other = if forward {
        scanner.forward(at, &graphemes, max_lines)
    } else {
        scanner.backward(at, &graphemes, max_lines)
    }?;
    Some(Matched {
        bracket: at,
        other,
        cursor_after,
    })
}

struct Scanner<'a> {
    buffer: &'a Buffer,
    open: &'a str,
    close: &'a str,
    depth: usize,
    budget: usize,
}

impl Scanner<'_> {
    /// Moves the depth for one grapheme; true when it is the match.
    fn step(&mut self, grapheme: &str, forward: bool) -> bool {
        let (deeper, shallower) = match forward {
            true => (self.open, self.close),
            false => (self.close, self.open),
        };
        if grapheme == deeper {
            self.depth += 1;
        } else if grapheme == shallower {
            self.depth -= 1;
        }
        self.depth == 0
    }

    fn forward(&mut self, at: Position, first: &[&str], max_lines: usize) -> Option<Position> {
        for (column, grapheme) in first.iter().enumerate().skip(at.column + 1) {
            self.budget = self.budget.checked_sub(1)?;
            if self.step(grapheme, true) {
                return Some(Position::new(at.line, column));
            }
        }
        let last = (at.line + max_lines).min(self.buffer.len_lines().saturating_sub(1));
        for line in at.line + 1..=last {
            let content = self.buffer.line_content(line).ok()?;
            if content.len() > MAX_LINE_BYTES {
                continue;
            }
            for (column, grapheme) in content.graphemes(true).enumerate() {
                self.budget = self.budget.checked_sub(1)?;
                if self.step(grapheme, true) {
                    return Some(Position::new(line, column));
                }
            }
        }
        None
    }

    fn backward(&mut self, at: Position, first: &[&str], max_lines: usize) -> Option<Position> {
        for column in (0..at.column).rev() {
            self.budget = self.budget.checked_sub(1)?;
            if self.step(first[column], false) {
                return Some(Position::new(at.line, column));
            }
        }
        let lowest = at.line.saturating_sub(max_lines);
        for line in (lowest..at.line).rev() {
            let content = self.buffer.line_content(line).ok()?;
            if content.len() > MAX_LINE_BYTES {
                continue;
            }
            let graphemes: Vec<&str> = content.graphemes(true).collect();
            for column in (0..graphemes.len()).rev() {
                self.budget = self.budget.checked_sub(1)?;
                if self.step(graphemes[column], false) {
                    return Some(Position::new(line, column));
                }
            }
        }
        None
    }
}

/// The bracket match of the primary cursor of `view`, when it is a bare cursor next to one.
pub(crate) fn at_cursor(buffer: &Buffer, view: &ViewState, max_lines: usize) -> Option<Matched> {
    let primary = view.selections().primary();
    if !primary.is_empty() {
        return None;
    }
    find(buffer, primary.head(), max_lines)
}

impl Editor {
    /// The bracket match of the primary cursor, when it is a bare cursor next to a bracket.
    pub(crate) fn matching_bracket(&self, max_lines: usize) -> Option<Matched> {
        at_cursor(&self.buffer, &self.view, max_lines)
    }

    /// Moves the cursor to the matching bracket, to the same side of it as it was of the
    /// first one, so doing it again comes back. `None` when there is no match; otherwise
    /// the place the cursor is now at.
    pub(crate) fn jump_to_matching_bracket(&mut self) -> Result<Option<Position>, BufferError> {
        let Some(found) = self.matching_bracket(JUMP_LINES) else {
            return Ok(None);
        };
        let to = match found.cursor_after {
            true => Position::new(found.other.line, found.other.column + 1),
            false => found.other,
        };
        self.view.last_edit = None::<EditKind>;
        self.view.selections = Selections::single(Selection::cursor(to));
        Ok(Some(to))
    }
}
