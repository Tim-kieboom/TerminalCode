//! What a TerminalCode plugin and the editor say to each other. A plugin is a
//! WebAssembly module (see "Plugin runtime" in `docs/design.md`); its messages
//! are JSON objects with a `type` field, and this crate is the one place that
//! says what they look like, for the editor and for the people who write plugins.
//!
//! **Compatibility.** Within one ABI major version changes are additive only:
//! a field is added, never removed or changed; a kind of message or of view is
//! added, never changed. So every enum has an `Unknown` catch-all and is
//! `#[non_exhaustive]` (a plugin must have an arm for what it does not know yet),
//! every struct is `#[non_exhaustive]` (build it with its `new`), a field that is
//! missing reads as its default, and a field that is not known is skipped. A
//! change that breaks an old plugin bumps the major version, and the editor
//! refuses a plugin built for another major (see [`Abi::check`]).

pub use abi::{Abi, AbiError};
pub use manifest::Manifest;
pub use message::{
    ActionDecl, Capability, FromPlugin, GitEntry, GitKind, GitSection, GitSummary, MessageError,
    Refusal, ToPlugin, WireLine, WireView,
};

mod abi;
mod manifest;
mod message;
#[cfg(test)]
mod tests;

/// The largest message either way, in bytes. A larger one is refused.
pub const MAX_MESSAGE: usize = 1024 * 1024;

/// The most entries one page of [`ToPlugin::GitEntries`] has.
pub const MAX_PAGE: usize = 500;
