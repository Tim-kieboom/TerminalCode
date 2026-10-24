//! A client for debug adapters, which speak the Debug Adapter Protocol
//! (<https://microsoft.github.io/debug-adapter-protocol/>): `lldb-dap`, and the others. It
//! starts an adapter (or takes the streams of one), sends it requests, and tells what it
//! does: events (the program stopped, ended), and requests of its own (start the program in a
//! terminal). It has no UI and no threads but the one that reads.
//!
//! See "Run configurations and a debugger" in `docs/design.md`.

mod client;
mod connection;
mod error;
mod framing;
mod message;
mod types;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use client::{Client, Update};
pub use connection::{Connection, Incoming, PendingReply};
pub use error::DapError;
pub use framing::{FrameError, MAX_BODY, read_message, write_message};
pub use message::{Message, MessageError, Rejection};
pub use types::*;

#[cfg(test)]
mod tests;
