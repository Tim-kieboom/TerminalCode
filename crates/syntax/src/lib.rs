//! Syntax highlighting: a document's text is parsed with tree-sitter and the
//! language's highlight query turns the tree into flat spans, each a byte
//! range with the style the theme gives its capture.

pub use captures::CaptureStyles;
pub use document::DocumentSyntax;
pub use error::SyntaxError;
pub use highlighter::{Highlighter, Span};
pub use language::{Comment, Language};
pub use plugin::{
    LanguageSpec, PluginSlot, register as register_language, unregister as unregister_language,
};
pub use ranges::{MAX_RANGES, merge as merge_ranges};
pub use worker::{Output, SyntaxWorker};

mod captures;
pub mod document;
mod error;
mod highlighter;
mod language;
mod mapping;
mod plugin;
mod ranges;
#[cfg(test)]
mod tests;
pub mod worker;
