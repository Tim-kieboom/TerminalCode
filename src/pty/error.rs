use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum PtyError {
    #[error("cannot open a pseudo-terminal: {0}")]
    Open(String),
    #[error("cannot start `{program}`: {message}")]
    Spawn { program: String, message: String },
}
