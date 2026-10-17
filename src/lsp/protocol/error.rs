use thiserror::Error;

use crate::lsp::message::RpcError;

#[derive(Debug, Error)]
pub(crate) enum LspError {
    /// The connection is over: the server exited, or the stream broke.
    #[error("the language server closed the connection: {0}")]
    Closed(String),
    #[error("the language server did not answer {method} within {seconds} s")]
    Timeout { method: String, seconds: u64 },
    #[error("the language server refused {method}: {error}")]
    Rejected { method: String, error: RpcError },
    #[error("the language server's answer to {method} is not usable: {detail}")]
    Unexpected { method: String, detail: String },
    #[error("the language server counts positions in {0}, and only utf-16 is supported")]
    Encoding(String),
    #[error("{0} cannot be written as a file URI")]
    NotAUri(String),
}
