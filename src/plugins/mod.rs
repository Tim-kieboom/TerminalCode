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
#[cfg(test)]
pub(crate) use runtime::fuel_for;
pub(crate) use runtime::{Notice, Runtime, WriteOp};

mod calls;
mod exec;
#[cfg(test)]
pub(crate) mod fixtures;
mod git_view;
pub(crate) mod index;
pub(crate) mod install;
mod instance;
pub(crate) mod manifest;
mod policy;
mod runs;
mod runtime;
#[cfg(test)]
pub mod tests;
pub(crate) mod trust;
mod view;

/// Fuel for one message the plugin is sent. About one unit for each WebAssembly
/// instruction. A placeholder until the Git viewer is measured: the plan is about
/// 50 times its slowest callback on a 10 000-entry status (todo, plugin step 6).
pub(crate) const CALLBACK_FUEL: u64 = 5_000_000;
/// Fuel for each byte of what a plugin on a thread of its own is sent to work on, on top
/// of [`CALLBACK_FUEL`]: the text of a file (`ToPlugin::FileText`), the payload of a call
/// or a reply (`Call`, `Reply`) and the text a command wrote (`RunOutput`). The rate is 3 times the worst the markdown
/// plugin spends on a byte of a dense document (about 5 480 fuel; `real_markdown_tests`
/// measures it again and fails when this falls under 3 times).
pub(crate) const FUEL_PER_BYTE: u64 = 16_600;
/// The most fuel one message may get, however big, on a thread of its own: 3 times what
/// the markdown plugin spends on the largest document it renders (128 KB), a few
/// seconds of one processor.
pub(crate) const OWNED_FUEL_CEILING: u64 = 2_200_000_000;
/// Fuel for loading: compiling is not charged, but the plugin's start function runs.
const LOAD_FUEL: u64 = 20_000_000;
/// How much memory a plugin may have, in bytes.
const MEMORY_LIMIT: usize = 16 * 1024 * 1024;

/// Runs a module on messages with a budget of `fuel` for each and says how much each used,
/// for measuring a real plugin (see `real_markdown_tests`).
#[cfg(test)]
pub(crate) fn measure_fuel(
    wasm: &[u8],
    fuel: u64,
    messages: Vec<crate::plugin_api::ToPlugin>,
) -> Vec<Result<u64, String>> {
    let compiled = instance::Compiled::new(wasm).expect("the module compiles");
    let mut instance = compiled.instantiate().expect("the module starts");
    messages
        .into_iter()
        .map(|message| {
            instance
                .deliver(message, fuel)
                .map(|delivered| delivered.fuel_used)
                .map_err(|failure| format!("{failure:?}"))
        })
        .collect()
}
