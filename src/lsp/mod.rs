//! A client for language servers (LSP): the framing of messages on a byte
//! stream, JSON-RPC requests and answers, and the handshake that starts a
//! session. Nothing here knows about one language or one server; see
//! "LSP client" in `docs/design.md`.
//!
//! Positions in this protocol are UTF-16 code units; the client offers nothing
//! else (see [`handshake::client_capabilities`]).

pub(crate) use connection::{Connection, Incoming, PendingReply};
pub(crate) use error::LspError;
pub(crate) use handshake::{Server, initialize, shutdown};
pub(crate) use message::{Id, Message, RpcError};

mod connection;
mod error;
mod framing;
mod handshake;
mod message;
mod replies;
#[cfg(test)]
mod tests;
pub(crate) mod uri;
