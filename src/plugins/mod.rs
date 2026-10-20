//! Plugins: WebAssembly modules run by `wasmi` on a thread of their own, that
//! talk to the editor by JSON messages. A plugin can draw into its pane and
//! nothing else yet; it cannot touch the editor, the files or the terminal. See
//! "Plugin runtime" in `docs/design.md`.
//!
//! A plugin module exports `memory`, `alloc(len) -> ptr` and `handle(ptr, len)`,
//! and imports `host.send(ptr, len)`. The editor writes a message into the
//! memory `alloc` gave and calls `handle`; the plugin answers by calling `send`
//! with messages of its own, once or more, before `handle` returns.
//!
//! Each call has a fuel budget (a runaway loop traps) and the memory has a limit.
//! A trap cannot be resumed: the instance is thrown away, a fresh one is started,
//! and a plugin that traps three times within a minute is disabled.

pub(crate) use crate::plugin_api::Thread;
pub(crate) use runtime::{Notice, Runtime, WriteOp};

#[cfg(test)]
pub(crate) mod fixtures;
mod git_view;
pub(crate) mod index;
pub(crate) mod install;
mod instance;
pub(crate) mod manifest;
mod policy;
mod runtime;
#[cfg(test)]
mod tests;
pub(crate) mod trust;
mod view;

/// Fuel for one message the plugin is sent. About one unit for each WebAssembly
/// instruction. A placeholder until the Git viewer is measured: the plan is about
/// 50 times its slowest callback on a 10 000-entry status (todo, plugin step 6).
const CALLBACK_FUEL: u64 = 5_000_000;
/// Fuel for each byte of the text of a file a plugin on a thread of its own is sent
/// (`ToPlugin::FileText`), on top of [`CALLBACK_FUEL`]. A placeholder until the
/// markdown plugin is measured: the plan is 3 times the worst measured (todo,
/// "Markdown preview", step 4).
const FUEL_PER_BYTE: u64 = 300;
/// The most fuel one message may get, however big, on a thread of its own: a few seconds.
const OWNED_FUEL_CEILING: u64 = 400_000_000;
/// Fuel for loading: compiling is not charged, but the plugin's start function runs.
const LOAD_FUEL: u64 = 20_000_000;
/// How much memory a plugin may have, in bytes.
const MEMORY_LIMIT: usize = 16 * 1024 * 1024;
