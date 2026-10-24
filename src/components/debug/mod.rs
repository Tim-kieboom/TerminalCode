//! The Debug pane: what the debugger shows (the model, which the session fills) and the pane that
//! draws it, with its keys and selection. The session itself is `app/debug.rs`.

mod hover;
pub(crate) mod model;
#[cfg(test)]
mod tests;
mod toolbar;
mod view;

pub(crate) use hover::DebugHoverBox;
pub(crate) use toolbar::Toolbar;
pub(crate) use view::{Counts, DebugCommand, DebugView, Section};
