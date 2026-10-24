use std::io;

use thiserror::Error;

use crate::message::Rejection;

#[derive(Debug, Error)]
pub enum DapError {
    /// The connection is over: the adapter exited, or the stream broke.
    #[error("the debug adapter closed the connection: {0}")]
    Closed(String),
    #[error("the debug adapter did not answer {command} within {seconds} s")]
    Timeout { command: String, seconds: u64 },
    #[error("the debug adapter refused {command}: {rejection}")]
    Rejected {
        command: String,
        rejection: Rejection,
    },
    #[error("the debug adapter's answer to {command} is not usable: {detail}")]
    Unexpected { command: String, detail: String },
    #[error("the debug adapter `{program}` could not be started: {source}")]
    Spawn {
        program: String,
        #[source]
        source: io::Error,
    },
}
