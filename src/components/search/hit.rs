//! One line of a file that matched the search, shaped for the results list.

use std::ops::Range;
use std::path::PathBuf;

use unicode_segmentation::UnicodeSegmentation;

/// A match further than this many bytes into the line (after the indent) is
/// shown with the start of the line cut off.
const FAR: usize = 60;
/// How much of the line before the match stays visible when it is cut.
const CONTEXT_BEFORE: usize = 40;
/// Longest line shown, in characters.
const MAX_CHARS: usize = 240;
const ELLIPSIS: &str = "…";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hit {
    /// From the project root.
    pub(crate) path: PathBuf,
    /// 0-based.
    pub(crate) line: usize,
    /// Grapheme column of the match in the file's line, where the cursor goes.
    pub(crate) column: usize,
    /// The line as shown: indent removed, long lines cut around the match.
    pub(crate) text: String,
    /// The match inside `text`, in bytes.
    pub(crate) matched: Range<usize>,
}

impl Hit {
    /// Builds the hit for a match at the byte range `found` of `line` (the
    /// text of the line, with or without its line break). `line_number` is
    /// 1-based.
    pub(crate) fn new(path: PathBuf, line_number: u64, line: &str, found: Range<usize>) -> Self {
        let line = line.trim_end_matches(['\r', '\n']);
        let start = floor_boundary(line, found.start.min(line.len()));
        let end = floor_boundary(line, found.end.min(line.len())).max(start);
        let column = line[..start].graphemes(true).count();

        // Drop the indent, unless the match is in it; cut a long lead-in.
        let indent = line.len() - line.trim_start().len();
        let mut from = indent.min(start);
        let mut prefix = "";
        if start - from > FAR {
            from = floor_boundary(line, start - CONTEXT_BEFORE);
            prefix = ELLIPSIS;
        }

        let (visible, cut) = truncate(&line[from..], MAX_CHARS);
        let mut text = String::with_capacity(prefix.len() + visible.len() + ELLIPSIS.len());
        text.push_str(prefix);
        text.push_str(visible);
        if cut {
            text.push_str(ELLIPSIS);
        }

        let shown_end = prefix.len() + visible.len();
        let matched_start = (prefix.len() + start - from).min(shown_end);
        let matched_end = (prefix.len() + end - from).clamp(matched_start, shown_end);
        Self {
            path,
            line: usize::try_from(line_number).unwrap_or(1).saturating_sub(1),
            column,
            text,
            matched: matched_start..matched_end,
        }
    }
}

/// The largest index at or before `index` that is a character boundary.
fn floor_boundary(text: &str, mut index: usize) -> usize {
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

/// The first `max` characters of `text`, and whether something was cut off.
fn truncate(text: &str, max: usize) -> (&str, bool) {
    match text.char_indices().nth(max) {
        Some((end, _)) => (&text[..end], true),
        None => (text, false),
    }
}
