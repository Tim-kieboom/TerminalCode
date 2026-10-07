use std::borrow::Cow;
use std::path::{Path, PathBuf};

use ropey::Rope;
use unicode_segmentation::UnicodeSegmentation;

pub use error::BufferError;
pub use file::FileError;

use edit::{AppliedEdit, Edit, EditInfo};
use history::History;
use line_ending::LineEnding;
use position::{Point, Position};

mod edit;
mod error;
mod file;
mod history;
mod line_ending;
mod position;
mod selection;
#[cfg(test)]
mod tests;

/// Text of one file. Lines end with `\n` or `\r\n`.
///
/// All changes go through [`Buffer::apply`], which bumps `version`; anything
/// computed from the text (highlighting, plugin snapshots) carries the
/// version it saw so stale results can be discarded.
#[derive(Debug, Default)]
pub(crate) struct Buffer {
    rope: Rope,
    version: u64,
    saved_version: u64,
    line_ending: LineEnding,
    path: Option<PathBuf>,
    history: History,
}

impl Buffer {
    pub(crate) fn from_text(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            version: 0,
            saved_version: 0,
            line_ending: LineEnding::detect(text),
            path: None,
            history: History::default(),
        }
    }

    pub(crate) fn version(&self) -> u64 {
        self.version
    }

    pub(crate) fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Whether the text differs from what was last opened or saved. Undoing
    /// back to the saved text still counts as modified.
    pub(crate) fn is_dirty(&self) -> bool {
        self.version != self.saved_version
    }

    pub(crate) fn len_bytes(&self) -> usize {
        self.rope.len_bytes()
    }

    /// Number of lines. An empty buffer has one empty line, and a trailing
    /// line break starts a new empty line.
    pub(crate) fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    pub(crate) fn text(&self) -> String {
        self.rope.to_string()
    }

    /// Applies `edit` and returns its inverse, bypassing history. Editing
    /// code goes through [`Buffer::begin_transaction`]. On error the buffer
    /// is unchanged.
    fn apply(&mut self, edit: &Edit) -> Result<AppliedEdit, BufferError> {
        let range = edit.range();
        self.check_range(range.start, range.end)?;

        let start_char = self.rope.byte_to_char(range.start);
        let end_char = self.rope.byte_to_char(range.end);
        let removed = self.rope.slice(start_char..end_char).to_string();
        let start_point = self.point_at(range.start);
        let old_end_point = self.point_at(range.end);

        self.rope.remove(start_char..end_char);
        self.rope.insert(start_char, edit.text());
        self.version += 1;

        let new_end_byte = range.start + edit.text().len();
        Ok(AppliedEdit {
            inverse: Edit::new(range.start..new_end_byte, removed),
            info: EditInfo {
                start_byte: range.start,
                old_end_byte: range.end,
                new_end_byte,
                start_point,
                old_end_point,
                new_end_point: self.point_at(new_end_byte),
            },
        })
    }

    /// Byte offset of `position`. A column equal to the line's grapheme count
    /// is valid and points just before the line break.
    pub(crate) fn position_to_byte(&self, position: Position) -> Result<usize, BufferError> {
        let line_start = self.line_start(position.line)?;
        let content = self.line_content(position.line)?;

        let mut graphemes = content.grapheme_indices(true);
        if let Some((offset, _)) = graphemes.nth(position.column) {
            return Ok(line_start + offset);
        }

        let columns = content.graphemes(true).count();
        if position.column != columns {
            return Err(BufferError::ColumnOutOfBounds {
                line: position.line,
                column: position.column,
                columns,
            });
        }
        Ok(line_start + content.len())
    }

    /// Position of `byte`. A byte inside a grapheme cluster maps to the start
    /// of that cluster, and a byte inside a line break maps to the end of
    /// its line.
    pub(crate) fn byte_to_position(&self, byte: usize) -> Result<Position, BufferError> {
        self.check_boundary(byte)?;

        let line = self.rope.byte_to_line(byte);
        let offset = byte - self.line_start(line)?;
        let content = self.line_content(line)?;

        let column = content
            .grapheme_indices(true)
            .take_while(|(start, grapheme)| start + grapheme.len() <= offset)
            .count();
        Ok(Position::new(line, column))
    }

    fn check_range(&self, start: usize, end: usize) -> Result<(), BufferError> {
        if start > end {
            return Err(BufferError::InvertedRange { start, end });
        }
        self.check_boundary(start)?;
        self.check_boundary(end)
    }

    fn check_boundary(&self, byte: usize) -> Result<(), BufferError> {
        let len = self.rope.len_bytes();
        if byte > len {
            return Err(BufferError::ByteOutOfBounds { offset: byte, len });
        }
        let char_index = self.rope.byte_to_char(byte);
        if self.rope.char_to_byte(char_index) != byte {
            return Err(BufferError::NotCharBoundary(byte));
        }
        Ok(())
    }

    fn point_at(&self, byte: usize) -> Point {
        let row = self.rope.byte_to_line(byte);
        Point {
            row,
            column: byte - self.rope.line_to_byte(row),
        }
    }

    fn line_start(&self, line: usize) -> Result<usize, BufferError> {
        let lines = self.rope.len_lines();
        if line >= lines {
            return Err(BufferError::LineOutOfBounds { line, lines });
        }
        Ok(self.rope.line_to_byte(line))
    }

    /// Text of `line` without its line break.
    fn line_content(&self, line: usize) -> Result<Cow<'_, str>, BufferError> {
        let lines = self.rope.len_lines();
        let Some(slice) = self.rope.get_line(line) else {
            return Err(BufferError::LineOutOfBounds { line, lines });
        };
        let text = Cow::from(slice);
        let content_len = strip_line_break(&text).len();
        Ok(match text {
            Cow::Borrowed(text) => Cow::Borrowed(&text[..content_len]),
            Cow::Owned(mut text) => {
                text.truncate(content_len);
                Cow::Owned(text)
            }
        })
    }
}

fn strip_line_break(line: &str) -> &str {
    let Some(line) = line.strip_suffix('\n') else {
        return line;
    };
    line.strip_suffix('\r').unwrap_or(line)
}
