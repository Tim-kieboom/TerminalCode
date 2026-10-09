//! Syntax highlighting: a document's text is parsed with tree-sitter and the
//! language's highlight query turns the tree into flat spans, each a byte
//! range with the style the theme gives its capture.

pub(crate) use document::DocumentSyntax;
pub(crate) use error::SyntaxError;
pub(crate) use highlighter::{Highlighter, Span};
pub(crate) use language::Language;
pub(crate) use ranges::{MAX_RANGES, merge as merge_ranges};

pub(crate) mod document;
mod error;
mod highlighter;
mod language;
mod mapping;
mod ranges;
#[cfg(test)]
mod tests;
