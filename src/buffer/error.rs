use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BufferError {
    #[error("byte offset {offset} is past the end of the buffer ({len} bytes)")]
    ByteOutOfBounds { offset: usize, len: usize },
    #[error("byte offset {0} is not on a character boundary")]
    NotCharBoundary(usize),
    #[error("edit range {start}..{end} is inverted")]
    InvertedRange { start: usize, end: usize },
    #[error("line {line} does not exist (buffer has {lines} lines)")]
    LineOutOfBounds { line: usize, lines: usize },
    #[error("column {column} is past the end of line {line} ({columns} columns)")]
    ColumnOutOfBounds {
        line: usize,
        column: usize,
        columns: usize,
    },
}
