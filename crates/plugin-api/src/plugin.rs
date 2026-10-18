//! What a plugin is, for whoever writes one in Rust: something that answers each
//! message the editor sends with the messages it has for the editor. It needs
//! nothing of WebAssembly, so it can be tested on the machine it is written on;
//! [`export_plugin!`](crate::export_plugin) makes the module out of it.

use crate::{FromPlugin, ToPlugin};

pub trait Plugin {
    /// Called for each message the editor sends; the messages it returns are sent
    /// to the editor, in order, before the call returns.
    fn handle(&mut self, message: ToPlugin) -> Vec<FromPlugin>;
}
