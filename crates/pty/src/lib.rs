//! A shell in a pseudo-terminal: the child process, the screen its output
//! draws, and the way keystrokes and pastes get to it without ever blocking
//! the thread that draws.

pub use error::PtyError;
pub use keys::{encode_key, encode_paste};
pub use mouse::encode_mouse;
pub use session::{Feeder, Session, SpawnConfig};
pub use shell::Shell;

mod error;
mod keys;
mod mouse;
mod queue;
mod replies;
mod session;
mod shell;
#[cfg(test)]
mod tests;
