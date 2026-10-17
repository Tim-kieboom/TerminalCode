//! A client for language servers (LSP): the framing of messages on a byte
//! stream, JSON-RPC requests and answers, and the handshake that starts a
//! session. Nothing here knows about one language or one server; see
//! "LSP client" in `docs/design.md`.
//!
//! Positions in this protocol are UTF-16 code units; the client offers nothing
//! else (see [`handshake::client_capabilities`]).

pub use completion::{
    CompletionItem, CompletionList, Kind, PendingCompletion, PendingResolve, Provider, TextEdit,
    Trigger, parse_completion,
};
pub use completion_state::{Accepted, ByteEdit, Completion, Refresh, byte_edits};
pub use config::{ConfigError, LanguageServers, ServerConfig, ServerKey, root_for};
pub use connection::{Connection, Incoming, PendingReply};
pub use definition::{Location, PendingDefinition, parse_definition};
pub use diagnostics::{Diagnostic, Diagnostics, RawDiagnostic, Severity, parse_published};
pub use diff::{ByteChange, change_between};
pub use error::LspError;
pub use handshake::{Server, initialize, shutdown};
pub use hover::{PendingHover, parse_hover};
pub use language::language_id_of;
pub use launch::{LaunchSpec, Launcher, ProcessLauncher, Session, StartError};
pub use manager::{Manager, ServerNotification};
pub use message::{Id, Message, RpcError};
pub use position::{LspPosition, from_lsp, to_lsp};
pub use supervisor::{Effect, Notice, Supervisor};
pub use sync::{DocSnapshot, DocumentSync, Outgoing, language_id};

mod features;
mod protocol;
mod session;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
#[cfg(test)]
mod tests;

// The files of each group are named from here, so `crate::hover` and the like
// keep working from inside the client.
use features::{completion, completion_state, definition, diagnostics, docs, hover, snippet};
pub use protocol::uri;
use protocol::{connection, error, framing, handshake, message, position, replies};
use session::{cargo, config, diff, language, launch, manager, progress, supervisor, sync};
