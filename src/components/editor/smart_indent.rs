//! Smart indent: what Enter and a typed closing bracket do beyond copying the
//! indentation, for the languages where brackets mark a level.
//!
//! The decision looks at the one line the cursor is on, skipping strings and a
//! trailing line comment. A bracket that opens on an earlier line inside a block
//! comment or a multi-line string is not noticed.

use std::path::Path;

use crate::buffer::Buffer;
use crate::syntax::{Comment, Language};

/// Brackets that open a level, with the one that closes it.
const PAIRS: [(char, char); 3] = [('{', '}'), ('(', ')'), ('[', ']')];

/// How a language writes its brackets and comments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Dialect {
    line_comment: &'static str,
}

impl Dialect {
    /// The dialect of the file at `path`; `None` for a file that gets no smart
    /// indent (unknown types, and Markdown, where brackets end prose lines).
    pub(super) fn of(path: &Path) -> Option<Self> {
        let line_comment = match Language::from_path(path)? {
            Language::Rust | Language::Json | Language::JavaScript => "//",
            Language::Toml | Language::Nix => "#",
            Language::Markdown => return None,
            // A language a plugin added: its own marker, or one no text has when it has none.
            language @ Language::Plugin(_) => match language.comment() {
                Some(Comment::Line(marker)) => marker,
                _ => "\u{0}",
            },
        };
        Some(Self { line_comment })
    }

    /// The dialect of the buffer's file, if it has a path and gets smart indent.
    pub(super) fn of_buffer(buffer: &Buffer) -> Option<Self> {
        Self::of(buffer.path()?)
    }

    /// The bracket that closes the level `before` (the text of the line left of
    /// the cursor) leaves open, if it ends with an opening bracket.
    pub(super) fn closer_to_add(self, before: &str) -> Option<char> {
        let last = self.last_code_char(before)?;
        PAIRS
            .iter()
            .find(|(open, _)| *open == last)
            .map(|(_, close)| *close)
    }

    /// The last character of `line` that is code: not whitespace, not inside a
    /// string and not in a trailing comment.
    fn last_code_char(self, line: &str) -> Option<char> {
        let mut last = None;
        let mut quote = None;
        let mut escaped = false;
        let mut rest = line;
        while let Some(c) = rest.chars().next() {
            let width = c.len_utf8();
            match quote {
                Some(open) => {
                    if escaped {
                        escaped = false;
                    } else if c == '\\' {
                        escaped = true;
                    } else if c == open {
                        quote = None;
                    }
                }
                None if rest.starts_with(self.line_comment) => break,
                // A quote inside a character literal does not open a string.
                None if rest.starts_with("'\"'") => {
                    last = Some('\'');
                    rest = &rest[3..];
                    continue;
                }
                None if c == '"' => {
                    quote = Some(c);
                    last = Some(c);
                }
                None if !c.is_whitespace() => last = Some(c),
                None => {}
            }
            rest = &rest[width..];
        }
        // A line that ends inside a string has no code at its end.
        last.filter(|_| quote.is_none())
    }
}

/// Whether `text` is one closing bracket.
pub(super) fn is_closer(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some(c), None) if PAIRS.iter().any(|(_, close)| *close == c)
    )
}
