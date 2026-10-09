use std::cmp::Reverse;
use std::ops::Range;

use ratatui::style::Style;
use tree_sitter::{InputEdit, Parser, Point, Query, QueryCursor, StreamingIterator, Tree};

use super::{Language, SyntaxError};
use crate::buffer::EditInfo;
use crate::ui::theme::Theme;

/// A stretch of text and how to draw it. The spans of one query never
/// overlap and are ordered by position; text without a span is drawn plain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Span {
    /// Byte offsets into the text.
    pub(crate) range: Range<usize>,
    pub(crate) style: Style,
}

/// Parses text in one language and says which parts of it to color.
///
/// Captures of the highlight query are turned into styles by the theme when
/// the highlighter is made, so a changed theme needs a new highlighter.
/// Captures the theme has no style for are left out, and the span of the
/// capture around them shows through.
pub(crate) struct Highlighter {
    language: Language,
    parser: Parser,
    query: Query,
    /// The style of each capture of `query`, by capture index.
    styles: Vec<Option<Style>>,
    tree: Option<Tree>,
}

impl std::fmt::Debug for Highlighter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Highlighter")
            .field("language", &self.language)
            .field("parsed", &self.tree.is_some())
            .finish_non_exhaustive()
    }
}

impl Highlighter {
    pub(crate) fn new(language: Language, theme: &Theme) -> Result<Self, SyntaxError> {
        let name = language.name();
        let grammar = language.grammar();
        let mut parser = Parser::new();
        parser
            .set_language(&grammar)
            .map_err(|source| SyntaxError::Grammar {
                language: name,
                source,
            })?;
        let query =
            Query::new(&grammar, language.highlights()).map_err(|source| SyntaxError::Query {
                language: name,
                source,
            })?;
        let styles = query
            .capture_names()
            .iter()
            .map(|capture| theme.syntax_style(capture))
            .collect();
        Ok(Self {
            language,
            parser,
            query,
            styles,
            tree: None,
        })
    }

    /// Parses `text` from scratch, replacing the tree of any earlier text.
    pub(crate) fn parse(&mut self, text: &str) -> Result<(), SyntaxError> {
        self.tree = self.parser.parse(text, None);
        self.parsed_or_error()
    }

    fn parsed_or_error(&self) -> Result<(), SyntaxError> {
        match self.tree {
            Some(_) => Ok(()),
            None => Err(SyntaxError::Parse {
                language: self.language.name(),
            }),
        }
    }

    /// Tells the tree about changes to the text it was parsed from, oldest
    /// first, each described against the text the one before left. The next
    /// [`Highlighter::reparse`] then only looks at what changed.
    pub(crate) fn edit(&mut self, edits: &[EditInfo]) {
        let Some(tree) = self.tree.as_mut() else {
            return;
        };
        for edit in edits {
            tree.edit(&InputEdit {
                start_byte: edit.start_byte,
                old_end_byte: edit.old_end_byte,
                new_end_byte: edit.new_end_byte,
                start_position: point(edit.start_point),
                old_end_position: point(edit.old_end_point),
                new_end_position: point(edit.new_end_point),
            });
        }
    }

    /// Parses `text` reusing the tree of the text before the changes given to
    /// [`Highlighter::edit`]. `text` must be the text those changes lead to; if
    /// the edits do not describe it exactly the tree is wrong, so when in
    /// doubt use [`Highlighter::parse`].
    pub(crate) fn reparse(&mut self, text: &str) -> Result<(), SyntaxError> {
        self.tree = self.parser.parse(text, self.tree.as_ref());
        self.parsed_or_error()
    }

    /// The spans inside `range`, for `text`, which must be what was last
    /// parsed. Spans that stick out of the range are cut at its edges. Empty
    /// until `text` has been parsed.
    pub(crate) fn spans(&self, text: &str, range: Range<usize>) -> Vec<Span> {
        let Some(tree) = &self.tree else {
            return Vec::new();
        };
        let range = range.start.min(text.len())..range.end.min(text.len());
        let mut cursor = QueryCursor::new();
        cursor.set_byte_range(range.clone());
        let mut found = Vec::new();
        let mut captures = cursor.captures(&self.query, tree.root_node(), text.as_bytes());
        while let Some((matched, index)) = captures.next() {
            let capture = matched.captures()[*index];
            let Some(style) = self.styles[capture.index as usize] else {
                continue;
            };
            let node = capture.node;
            found.push(Found {
                start: node.start_byte(),
                end: node.end_byte(),
                pattern: matched.pattern_index,
                style,
            });
        }
        flatten(found, &range)
    }
}

/// One capture of the query that has a style.
#[derive(Debug, Clone, Copy)]
pub(super) struct Found {
    pub(super) start: usize,
    pub(super) end: usize,
    /// Which pattern of the query matched; the first one wins a tie.
    pub(super) pattern: usize,
    pub(super) style: Style,
}

/// Turns captures, which nest and may share a range, into spans that do not
/// overlap: the innermost capture wins where they nest, and the first
/// pattern wins where two cover exactly the same bytes. The result is cut to
/// `range`, and neighbours with the same style are joined.
pub(super) fn flatten(mut found: Vec<Found>, range: &Range<usize>) -> Vec<Span> {
    // Outer before inner; for equal ranges the earlier pattern first.
    found.sort_by_key(|f| (f.start, Reverse(f.end), f.pattern));
    found.dedup_by(|later, earlier| later.start == earlier.start && later.end == earlier.end);

    let mut out = Vec::new();
    // The captures that cover the current position, outermost first.
    let mut open: Vec<Found> = Vec::new();
    // Everything before this offset has been emitted.
    let mut at = range.start;
    let mut emit = |from: usize, to: usize, style: Style| {
        let (from, to) = (from.max(range.start), to.min(range.end));
        if from >= to {
            return;
        }
        match out.last_mut() {
            Some(Span { range, style: last }) if range.end == from && *last == style => {
                range.end = to;
            }
            _ => out.push(Span {
                range: from..to,
                style,
            }),
        }
    };

    for next in found {
        while let Some(top) = open.last().copied() {
            if top.end > next.start {
                break;
            }
            emit(at, top.end, top.style);
            at = at.max(top.end);
            open.pop();
        }
        if let Some(top) = open.last() {
            emit(at, next.start, top.style);
        }
        at = at.max(next.start);
        // A capture cannot run past the one around it.
        let end = open.last().map_or(next.end, |top| next.end.min(top.end));
        open.push(Found { end, ..next });
    }
    while let Some(top) = open.pop() {
        emit(at, top.end, top.style);
        at = at.max(top.end);
    }
    out
}

/// A buffer position as tree-sitter wants it.
fn point(at: crate::buffer::Point) -> Point {
    Point {
        row: at.row,
        column: at.column,
    }
}
