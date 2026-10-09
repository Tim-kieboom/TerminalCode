//! A shell in a pseudo-terminal: the child process, the screen its output
//! draws, and the way keystrokes and pastes get to it without ever blocking
//! the thread that draws.

pub(crate) use error::PtyError;
pub(crate) use keys::{encode_key, encode_paste};
pub(crate) use session::{Session, SpawnConfig};
pub(crate) use shell::Shell;

mod error;
mod keys;
mod queue;
mod session;
mod shell;
#[cfg(test)]
mod tests;
