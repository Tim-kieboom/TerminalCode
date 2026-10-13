//! A client for language servers (LSP): the framing of messages on a byte
//! stream, JSON-RPC requests and answers, and the handshake that starts a
//! session. Nothing here knows about one language or one server; see
//! "LSP client" in `docs/design.md`.
//!
//! Positions in this protocol are UTF-16 code units; the client offers nothing
//! else (see [`handshake::client_capabilities`]).

pub(crate) use completion::{
    CompletionItem, CompletionList, Kind, PendingCompletion, PendingResolve, Provider, TextEdit,
    Trigger, parse_completion,
};
pub(crate) use completion_state::{Accepted, ByteEdit, Completion, Refresh, byte_edits};
pub(crate) use config::{ConfigError, LanguageServers, ServerConfig, ServerKey};
pub(crate) use connection::{Connection, Incoming, PendingReply};
pub(crate) use definition::{Location, PendingDefinition, parse_definition};
pub(crate) use diagnostics::{Diagnostic, Diagnostics, RawDiagnostic, Severity, parse_published};
pub(crate) use diff::{ByteChange, change_between};
pub(crate) use error::LspError;
pub(crate) use handshake::{Server, initialize, shutdown};
pub(crate) use hover::{PendingHover, parse_hover};
pub(crate) use launch::{LaunchSpec, Launcher, ProcessLauncher, Session, StartError};
pub(crate) use manager::{Manager, ServerNotification};
pub(crate) use message::{Id, Message, RpcError};
pub(crate) use position::{LspPosition, to_lsp};
pub(crate) use supervisor::{Effect, Notice, Supervisor};
pub(crate) use sync::{DocSnapshot, DocumentSync, Outgoing, language_id};

mod completion;
mod completion_state;
mod config;
mod connection;
mod definition;
mod diagnostics;
mod diff;
mod docs;
mod error;
mod framing;
mod handshake;
mod hover;
mod launch;
mod manager;
mod message;
mod position;
mod progress;
mod replies;
mod snippet;
mod supervisor;
mod sync;
#[cfg(test)]
pub(crate) mod tests;
pub(crate) mod uri;
