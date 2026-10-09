//! Syntax highlighting: a document's text is parsed with tree-sitter and the
//! language's highlight query turns the tree into flat spans, each a byte
//! range with the style the theme gives its capture.

// Not used by the editor yet (M5 step B).
#![allow(dead_code, unused_imports)]

pub(crate) use error::SyntaxError;
pub(crate) use highlighter::{Highlighter, Span};
pub(crate) use language::Language;

mod error;
mod highlighter;
mod language;
#[cfg(test)]
mod tests;
