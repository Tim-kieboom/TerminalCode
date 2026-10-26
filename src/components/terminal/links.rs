//! Paths in the output of a shell: `src/main.rs:12:5`, rustc's `--> src/lib.rs:3:9`, a `file:line` of
//! a stack trace. The text under a cell is read from the screen (the rows that wrapped are joined),
//! the path in it is cut out, and the app looks if the file is there before it is a link.

use super::{Cell, TerminalPane};

/// How many rows are joined above and below the row of the pointer.
const MOST_WRAPPED_ROWS: u16 = 4;

/// A path in a text, with the place in the file it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinkText {
    pub(crate) path: String,
    /// From 1.
    pub(crate) line: Option<usize>,
    /// From 1.
    pub(crate) column: Option<usize>,
    /// The characters it covers, `start` up to `end`.
    pub(crate) start: usize,
    pub(crate) end: usize,
}

/// A path found under the pointer, with where it is on the screen (for the underline).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Link {
    pub(crate) text: LinkText,
    pub(crate) cells: Vec<Cell>,
}

/// What ends a path: a space, a quote, or a bracket around it.
fn is_delimiter(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '"' | '\'' | '`' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | ',' | ';'
        )
}

/// The path, and the line and column after it, that the character at `at` is a part of.
/// Punctuation after the path (`.`, `:`, `!`) is not part of it; a URL is not a path.
pub(crate) fn find_link(chars: &[char], at: usize) -> Option<LinkText> {
    if at >= chars.len() || is_delimiter(chars[at]) {
        return None;
    }
    let mut start = at;
    while start > 0 && !is_delimiter(chars[start - 1]) {
        start -= 1;
    }
    let mut end = at + 1;
    while end < chars.len() && !is_delimiter(chars[end]) {
        end += 1;
    }
    while end > start && matches!(chars[end - 1], '.' | ':' | '!' | '?') {
        end -= 1;
    }
    if at >= end {
        return None;
    }
    let token: String = chars[start..end].iter().collect();
    if token.contains("://") {
        return None;
    }
    let (path, line, column) = split_place(&token);
    let looks_like_a_path = path.contains(['/', '\\']) || has_extension(path);
    (looks_like_a_path && !path.is_empty()).then(|| LinkText {
        path: path.to_owned(),
        line,
        column,
        start,
        end,
    })
}

/// `src/main.rs:12:5` as the path, the line and the column; a drive (`C:\code`) is left alone.
fn split_place(token: &str) -> (&str, Option<usize>, Option<usize>) {
    let number_after = |text: &str| -> Option<(usize, usize)> {
        let colon = text.rfind(':')?;
        let digits = &text[colon + 1..];
        let number = (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
            .then(|| digits.parse().ok())
            .flatten()?;
        Some((colon, number))
    };
    let Some((first_colon, last)) = number_after(token) else {
        return (token, None, None);
    };
    match number_after(&token[..first_colon]) {
        Some((second_colon, line)) => (&token[..second_colon], Some(line), Some(last)),
        None => (&token[..first_colon], Some(last), None),
    }
}

/// Whether the name ends in a dot and letters or digits (`main.rs`, `Cargo.toml`).
fn has_extension(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.').is_some_and(|(stem, extension)| {
        !stem.is_empty()
            && !extension.is_empty()
            && extension.chars().all(|c| c.is_ascii_alphanumeric())
            && extension.chars().any(|c| c.is_ascii_alphabetic())
    })
}

impl TerminalPane {
    /// The path under a cell of the screen, if the text there has one. The rows that wrapped
    /// into each other are read as one line.
    pub(crate) fn link_at(&self, at: Cell) -> Option<Link> {
        let session = self.session()?;
        session.with_screen(|screen| {
            let (rows, columns) = screen.size();
            if at.row >= rows {
                return None;
            }
            let mut first = at.row;
            while first > 0 && at.row - first < MOST_WRAPPED_ROWS && screen.row_wrapped(first - 1) {
                first -= 1;
            }
            let mut last = at.row;
            while last + 1 < rows && last - at.row < MOST_WRAPPED_ROWS && screen.row_wrapped(last) {
                last += 1;
            }
            let mut chars = Vec::new();
            let mut cells = Vec::new();
            let mut index = None;
            for row in first..=last {
                for column in 0..columns {
                    let Some(cell) = screen.cell(row, column) else {
                        continue;
                    };
                    if cell.is_wide_continuation() {
                        continue;
                    }
                    if row == at.row && column <= at.column {
                        index = Some(chars.len());
                    }
                    chars.push(cell.contents().chars().next().unwrap_or(' '));
                    cells.push(Cell { row, column });
                }
            }
            let text = find_link(&chars, index?)?;
            // The pointer was on the cell, not past the end of the row's text.
            let covered = cells[text.start..text.end].to_vec();
            covered.contains(&at).then_some(Link {
                cells: covered,
                text,
            })
        })
    }

    /// The path the pointer is over while `ctrl` is held, underlined.
    pub(crate) fn link(&self) -> Option<&LinkMark> {
        self.link.as_ref().filter(|mark| {
            mark.tab == self.active
                && self.session().map(crate::pty::Session::scrollback_offset) == Some(mark.offset)
        })
    }

    /// Underlines the cells of a link that is there; `None` takes the underline away. Returns
    /// whether that changed anything.
    pub(crate) fn set_link(&mut self, cells: Option<Vec<Cell>>) -> bool {
        let mark = cells.and_then(|cells| {
            self.session().map(|session| LinkMark {
                tab: self.active,
                offset: session.scrollback_offset(),
                cells,
            })
        });
        let changed = self.link.as_ref().map(|m| &m.cells) != mark.as_ref().map(|m| &m.cells);
        self.link = mark;
        changed
    }
}

/// Where the underlined link is: on one tab, at one place in its history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LinkMark {
    tab: usize,
    offset: usize,
    pub(crate) cells: Vec<Cell>,
}
