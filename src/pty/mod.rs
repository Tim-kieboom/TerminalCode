//! A shell in a pseudo-terminal: the child process, the screen its output
//! draws, and the way keystrokes and pastes get to it without ever blocking
//! the thread that draws.

// Writing to the shell is used by the key handling (M6 step C).
#![allow(dead_code)]

pub(crate) use error::PtyError;
pub(crate) use session::{Session, SpawnConfig};
pub(crate) use shell::Shell;

mod error;
mod queue;
mod session;
mod shell;
#[cfg(test)]
mod tests;
