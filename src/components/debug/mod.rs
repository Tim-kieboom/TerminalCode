//! The Debug pane: what the debugger shows (the model, which the session fills) and the pane that
//! draws it, with its keys and selection. The session itself is `app/debug.rs`.

pub(crate) mod model;
#[cfg(test)]
mod tests;
mod view;

pub(crate) use view::{DebugCommand, DebugView, Section};
