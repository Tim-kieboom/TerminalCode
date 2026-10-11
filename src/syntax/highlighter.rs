use std::cmp::Reverse;
use std::ops::Range;

use ratatui::style::Style;
use ropey::Rope;
use ropey::iter::Chunks;
use tree_sitter::{
    InputEdit, Language as Grammar, Node, Parser, Point, Query, QueryCursor, StreamingIterator,
    TextProvider, Tree,
};

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
    /// For a language that holds another one (Markdown paragraphs).
    inner: Option<InnerHighlights>,
    tree: Option<Tree>,
}

/// What highlights the language inside a language: the node kind of the outer
/// tree whose text it covers, and how to parse and color that text.
struct InnerHighlights {
    node_kind: &'static str,
    grammar: Grammar,
    query: Query,
    styles: Vec<Option<Style>>,
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
            Query::new(&grammar, &language.highlights()).map_err(|source| SyntaxError::Query {
                language: name,
                source,
            })?;

        let styles = styles_of(&query, theme);
        let inner = language
            .inner()
            .map(|inner| -> Result<_, SyntaxError> {
                let query = Query::new(&inner.grammar, inner.highlights).map_err(|source| {
                    SyntaxError::Query {
                        language: name,
                        source,
                    }
                })?;
                Ok(InnerHighlights {
                    node_kind: inner.node_kind,
                    styles: styles_of(&query, theme),
                    query,
                    grammar: inner.grammar,
                })
            })
            .transpose()?;

        Ok(Self {
            language,
            parser,
            query,
            styles,
            inner,
            tree: None,
        })
    }

    /// Parses `text` from scratch, replacing the tree of any earlier text.
    pub(crate) fn parse(&mut self, text: &Rope) -> Result<(), SyntaxError> {
        self.tree = parse_rope(&mut self.parser, text, None);
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
    pub(crate) fn reparse(&mut self, text: &Rope) -> Result<(), SyntaxError> {
        self.tree = parse_rope(&mut self.parser, text, self.tree.as_ref());
        self.parsed_or_error()
    }

    /// The spans inside `range`, for `text`, which must be what was last
    /// parsed. Spans that stick out of the range are cut at its edges. Empty
    /// until `text` has been parsed.
    pub(crate) fn spans(&self, text: &Rope, range: Range<usize>) -> Vec<Span> {
        let Some(tree) = &self.tree else {
            return Vec::new();
        };
        let range = range.start.min(text.len_bytes())..range.end.min(text.len_bytes());
        let mut found = Vec::new();
        collect(
            &self.query,
            &self.styles,
            tree.root_node(),
            text,
            &range,
            0,
            &mut found,
        );
        if let Some(inner) = &self.inner {
            inner.collect_into(tree.root_node(), text, &range, &mut found);
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

/// Parses a rope without copying it: tree-sitter asks for the text in pieces.
fn parse_rope(parser: &mut Parser, text: &Rope, old: Option<&Tree>) -> Option<Tree> {
    let mut chunk_at = |byte: usize, _: Point| -> &[u8] {
        if byte >= text.len_bytes() {
            return &[];
        }
        let (chunk, start, ..) = text.chunk_at_byte(byte);
        &chunk.as_bytes()[byte - start..]
    };
    parser.parse_with_options(&mut chunk_at, old, None)
}

/// The text of a node, for the query's text predicates.
struct RopeText<'a>(&'a Rope);

impl<'a> TextProvider<&'a [u8]> for RopeText<'a> {
    type I = std::iter::Map<Chunks<'a>, fn(&str) -> &[u8]>;

    fn text(&mut self, node: Node) -> Self::I {
        self.0
            .byte_slice(node.byte_range())
            .chunks()
            .map(str::as_bytes)
    }
}

/// The style the theme gives each capture of `query`, by capture index.
fn styles_of(query: &Query, theme: &Theme) -> Vec<Option<Style>> {
    query
        .capture_names()
        .iter()
        .map(|capture| theme.syntax_style(capture))
        .collect()
}

/// Runs `query` over the part of the tree inside `range` and adds the styled
/// captures to `found`; `pattern_offset` keeps the patterns of one query
/// after those of another when they tie.
fn collect(
    query: &Query,
    styles: &[Option<Style>],
    root: Node<'_>,
    text: &Rope,
    range: &Range<usize>,
    pattern_offset: usize,
    found: &mut Vec<Found>,
) {
    let mut cursor = QueryCursor::new();
    cursor.set_byte_range(range.clone());
    let mut captures = cursor.captures(query, root, RopeText(text));
    while let Some((matched, index)) = captures.next() {
        let capture = matched.captures()[*index];
        let Some(style) = styles[capture.index as usize] else {
            continue;
        };
        let node = capture.node;
        found.push(Found {
            start: node.start_byte(),
            end: node.end_byte(),
            pattern: pattern_offset + matched.pattern_index,
            style,
        });
    }
}

/// Inner patterns come after every outer one when two cover the same bytes.
const INNER_PATTERN_OFFSET: usize = 1_000_000;

impl InnerHighlights {
    /// Parses the text of every `node_kind` node of `outer` that touches
    /// `range` with the inner grammar and adds what the inner query finds.
    /// Nothing is kept between calls: only a screenful is parsed each time.
    fn collect_into(
        &self,
        outer: Node<'_>,
        text: &Rope,
        range: &Range<usize>,
        found: &mut Vec<Found>,
    ) {
        let mut nodes = Vec::new();
        nodes_of_kind(outer, self.node_kind, range, &mut nodes);
        if nodes.is_empty() {
            return;
        }
        let mut parser = Parser::new();
        if parser.set_language(&self.grammar).is_err() {
            return;
        }
        for node in nodes {
            if parser.set_included_ranges(&[node.range()]).is_err() {
                continue;
            }
            let Some(tree) = parse_rope(&mut parser, text, None) else {
                continue;
            };
            let within = range.start.max(node.start_byte())..range.end.min(node.end_byte());
            collect(
                &self.query,
                &self.styles,
                tree.root_node(),
                text,
                &within,
                INNER_PATTERN_OFFSET,
                found,
            );
        }
    }
}

/// The nodes of `kind` under `root` that touch `range`, in order. Nodes
/// inside a node of that kind are not looked at.
fn nodes_of_kind<'t>(root: Node<'t>, kind: &str, range: &Range<usize>, out: &mut Vec<Node<'t>>) {
    let mut cursor = root.walk();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node.end_byte() <= range.start || node.start_byte() >= range.end {
            continue;
        }
        if node.kind() == kind {
            out.push(node);
            continue;
        }
        let before = pending.len();
        pending.extend(node.children(&mut cursor));
        // Popped from the end, so reverse to visit the children in order.
        pending[before..].reverse();
    }
}
