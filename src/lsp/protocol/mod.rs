//! The wire: framing, JSON-RPC messages, the connection and the handshake.

pub(super) mod connection;
pub(super) mod error;
pub(super) mod framing;
pub(super) mod handshake;
pub(super) mod message;
pub(super) mod position;
pub(super) mod replies;
pub(crate) mod uri;
