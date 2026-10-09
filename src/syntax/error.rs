use thiserror::Error;
use tree_sitter::{LanguageError, QueryError};

#[derive(Debug, Error)]
pub(crate) enum SyntaxError {
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
    #[error("the {language} parser gave up on the text")]
    Parse { language: &'static str },
}
