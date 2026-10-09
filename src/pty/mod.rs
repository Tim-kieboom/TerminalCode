//! A shell in a pseudo-terminal: the child process, the screen its output
//! draws, and the way keystrokes and pastes get to it without ever blocking
//! the thread that draws.

// Not used by the app yet (M6 step B).
#![allow(dead_code, unused_imports)]

pub(crate) use error::PtyError;
pub(crate) use session::{Session, SpawnConfig, Written};
pub(crate) use shell::Shell;

mod error;
mod queue;
mod session;
mod shell;
#[cfg(test)]
mod tests;
