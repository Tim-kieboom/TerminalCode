/// A place in a buffer: zero-based line and zero-based grapheme column.
///
/// The column counts grapheme clusters, so one cursor stop is one thing the
/// user sees as a character. It does not include the line break: the largest
/// valid column of a line is its grapheme count.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// A place in a buffer as tree-sitter expects it: zero-based row and byte
/// column within that row.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Point {
    pub row: usize,
    pub column: usize,
}
