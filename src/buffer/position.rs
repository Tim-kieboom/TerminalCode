/// A place in a buffer: zero-based line and zero-based grapheme column.
///
/// The column counts grapheme clusters, so one cursor stop is one thing the
/// user sees as a character. It does not include the line break: the largest
/// valid column of a line is its grapheme count.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Position {
    pub(crate) line: usize,
    pub(crate) column: usize,
}

impl Position {
    pub(crate) fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// A place in a buffer as tree-sitter expects it: zero-based row and byte
/// column within that row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Point {
    pub(crate) row: usize,
    pub(crate) column: usize,
}
