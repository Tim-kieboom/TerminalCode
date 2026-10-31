use thiserror::Error;
use tree_sitter::{LanguageError, QueryError, WasmError};

#[derive(Debug, Error)]
pub enum SyntaxError {
    #[error("the {language} grammar does not fit this version of tree-sitter: {source}")]
    Grammar {
        language: &'static str,
        source: LanguageError,
    },
    #[error("the {language} highlight query is not valid: {source}")]
    Query {
        language: &'static str,
        source: QueryError,
    },
    #[error("the {language} grammar could not be loaded: {}", .source.message)]
    Wasm {
        language: &'static str,
        source: WasmError,
    },
    #[error("the language of this file was taken away")]
    Gone,
    #[error("the {language} parser gave up on the text")]
    Parse { language: &'static str },
}
