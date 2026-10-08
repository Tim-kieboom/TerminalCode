use serde::Deserialize;
use unicode_segmentation::UnicodeSegmentation;

use crate::buffer::{Buffer, BufferError, Position};

/// A cursor movement, shared by plain movement and selection extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    PageUp,
    PageDown,
    LineStart,
    LineEnd,
    DocumentStart,
    DocumentEnd,
}

/// Where a motion lands. `desired_column` is only set by vertical motions.
pub(super) struct Target {
    pub(super) position: Position,
    pub(super) desired_column: Option<usize>,
}

impl Target {
    fn at(line: usize, column: usize) -> Self {
        Self {
            position: Position::new(line, column),
            desired_column: None,
        }
    }
}

pub(super) fn target(
    buffer: &Buffer,
    from: Position,
    desired_column: Option<usize>,
    motion: Motion,
    page: usize,
) -> Result<Target, BufferError> {
    let last_line = buffer.len_lines() - 1;
    match motion {
        Motion::Left => left(buffer, from),
        Motion::Right => right(buffer, from, last_line),
        Motion::Up => up(buffer, from, desired_column),
        Motion::Down => down(buffer, from, desired_column, last_line),
        Motion::PageUp => page_up(buffer, from, desired_column, page),
        Motion::PageDown => page_down(buffer, from, desired_column, page, last_line),
        Motion::WordLeft => word_left(buffer, from),
        Motion::WordRight => word_right(buffer, from, last_line),
        Motion::LineStart => Ok(Target::at(from.line, 0)),
        Motion::LineEnd => Ok(Target::at(from.line, buffer.line_len(from.line)?)),
        Motion::DocumentStart => Ok(Target::at(0, 0)),
        Motion::DocumentEnd => Ok(Target::at(last_line, buffer.line_len(last_line)?)),
    }
}

fn left(buffer: &Buffer, from: Position) -> Result<Target, BufferError> {
    if from.column > 0 {
        return Ok(Target::at(from.line, from.column - 1));
    }
    let Some(line) = from.line.checked_sub(1) else {
        return Ok(Target::at(0, 0));
    };
    Ok(Target::at(line, buffer.line_len(line)?))
}

fn right(buffer: &Buffer, from: Position, last_line: usize) -> Result<Target, BufferError> {
    let len = buffer.line_len(from.line)?;
    if from.column < len {
        return Ok(Target::at(from.line, from.column + 1));
    }
    if from.line < last_line {
        return Ok(Target::at(from.line + 1, 0));
    }
    Ok(Target::at(from.line, len))
}

fn up(
    buffer: &Buffer,
    from: Position,
    desired_column: Option<usize>,
) -> Result<Target, BufferError> {
    let Some(line) = from.line.checked_sub(1) else {
        return Ok(Target::at(0, 0));
    };
    vertical(buffer, from, desired_column, line)
}

fn down(
    buffer: &Buffer,
    from: Position,
    desired_column: Option<usize>,
    last_line: usize,
) -> Result<Target, BufferError> {
    if from.line >= last_line {
        return Ok(Target::at(last_line, buffer.line_len(last_line)?));
    }
    vertical(buffer, from, desired_column, from.line + 1)
}

/// `page` lines up, or the document start when already on the first line.
fn page_up(
    buffer: &Buffer,
    from: Position,
    desired_column: Option<usize>,
    page: usize,
) -> Result<Target, BufferError> {
    if from.line == 0 {
        return Ok(Target::at(0, 0));
    }
    vertical(buffer, from, desired_column, from.line.saturating_sub(page))
}

/// `page` lines down, or the document end when already on the last line.
fn page_down(
    buffer: &Buffer,
    from: Position,
    desired_column: Option<usize>,
    page: usize,
    last_line: usize,
) -> Result<Target, BufferError> {
    if from.line >= last_line {
        return Ok(Target::at(last_line, buffer.line_len(last_line)?));
    }
    vertical(
        buffer,
        from,
        desired_column,
        (from.line + page).min(last_line),
    )
}

/// What a grapheme counts as for word motion.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Space,
    Word,
    Punctuation,
}

fn classify(grapheme: &str) -> CharClass {
    let Some(first) = grapheme.chars().next() else {
        return CharClass::Space;
    };
    if first.is_whitespace() {
        CharClass::Space
    } else if first.is_alphanumeric() || first == '_' {
        CharClass::Word
    } else {
        CharClass::Punctuation
    }
}

/// Start of the word before the cursor: skips spaces, then one run of the
/// same class. At the start of a line it moves to the end of the previous one.
fn word_left(buffer: &Buffer, from: Position) -> Result<Target, BufferError> {
    if from.column == 0 {
        return left(buffer, from);
    }
    let content = buffer.line_content(from.line)?;
    let graphemes: Vec<&str> = content.graphemes(true).collect();

    let mut column = from.column.min(graphemes.len());
    while column > 0 && classify(graphemes[column - 1]) == CharClass::Space {
        column -= 1;
    }
    if column > 0 {
        let class = classify(graphemes[column - 1]);
        while column > 0 && classify(graphemes[column - 1]) == class {
            column -= 1;
        }
    }
    Ok(Target::at(from.line, column))
}

/// End of the word after the cursor: skips spaces, then one run of the same
/// class. At the end of a line it moves to the start of the next one.
fn word_right(buffer: &Buffer, from: Position, last_line: usize) -> Result<Target, BufferError> {
    let content = buffer.line_content(from.line)?;
    let graphemes: Vec<&str> = content.graphemes(true).collect();
    if from.column >= graphemes.len() {
        return right(buffer, from, last_line);
    }

    let mut column = from.column;
    while column < graphemes.len() && classify(graphemes[column]) == CharClass::Space {
        column += 1;
    }
    if column < graphemes.len() {
        let class = classify(graphemes[column]);
        while column < graphemes.len() && classify(graphemes[column]) == class {
            column += 1;
        }
    }
    Ok(Target::at(from.line, column))
}

/// Moves to `line`, aiming for the remembered column but stopping at the end
/// of a shorter line.
fn vertical(
    buffer: &Buffer,
    from: Position,
    desired_column: Option<usize>,
    line: usize,
) -> Result<Target, BufferError> {
    let wanted = desired_column.unwrap_or(from.column);
    let column = wanted.min(buffer.line_len(line)?);
    Ok(Target {
        position: Position::new(line, column),
        desired_column: Some(wanted),
    })
}
