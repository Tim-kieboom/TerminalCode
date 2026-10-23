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
pub use manifest::{Manifest, Thread};
pub use message::{
    ActionDecl, Capability, FromPlugin, GitEntry, GitKind, GitSection, GitSummary, MessageError,
    Refusal, RunStream, ToPlugin, WireButton, WireLine, WireMenuItem, WireSpan, WireView,
};
pub use plugin::Plugin;
pub use run::{PatternError, RunPattern, allows, parse_all};

mod abi;
pub mod guest;
mod manifest;
mod message;
mod plugin;
mod run;
#[cfg(test)]
mod tests;

/// The largest message either way, in bytes. A larger one is refused.
///
/// A [`ToPlugin::FileText`] has up to [`MAX_FILE_TEXT`] bytes of text and JSON
/// writes some of it longer (a line break is two bytes), so this is a few times that.
pub const MAX_MESSAGE: usize = 4 * 1024 * 1024;

/// The most text of a file the editor sends a plugin ([`ToPlugin::FileText`]); a larger
/// file is told as [`ToPlugin::FileTooLarge`].
pub const MAX_FILE_TEXT: usize = 1024 * 1024;

/// The most entries one page of [`ToPlugin::GitEntries`] has.
pub const MAX_PAGE: usize = 500;

/// The most calls one plugin may have open at once ([`FromPlugin::Call`] that is not
/// answered to the last part, cancelled or failed); a call over it is refused as
/// [`Refusal::Busy`].
pub const MAX_OPEN_CALLS: usize = 64;

/// The most commands one plugin may have running at once; a command over it is
/// refused as [`Refusal::Busy`].
pub const MAX_OPEN_RUNS: usize = 4;

/// The most bytes of text one [`ToPlugin::RunOutput`] has. Longer output comes in
/// more than one.
pub const MAX_RUN_CHUNK: usize = 64 * 1024;

/// The most output (both streams together) the editor passes on for one command; the
/// rest is read and dropped, and [`ToPlugin::RunDone`] says `truncated`.
pub const MAX_RUN_OUTPUT: usize = 32 * 1024 * 1024;

/// The most items a menu a plugin shows may have ([`FromPlugin::Menu`]); the rest are left out.
pub const MAX_MENU_ITEMS: usize = 24;
