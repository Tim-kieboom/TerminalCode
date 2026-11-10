//! What a server says about itself, kept to be looked at: the lines of its error output, its
//! `window/logMessage` and `window/showMessage`, and what happened to it (started, ended,
//! restarted). One buffer is kept for each server for as long as the editor runs, so it holds
//! the history across restarts.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// How many lines are kept; the oldest go first.
const MOST_LINES: usize = 5000;

/// A buffer of log lines that can be written from the threads that read a server and read by the
/// app. Clones are the same buffer.
#[derive(Clone, Default)]
pub struct LogSink {
    lines: Arc<Mutex<VecDeque<String>>>,
}

impl LogSink {
    /// Adds a line (a line that has several lines in it becomes several).
    pub fn push(&self, text: &str) {
        let mut lines = self
            .lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for line in text.lines() {
            if lines.len() >= MOST_LINES {
                lines.pop_front();
            }
            lines.push_back(line.trim_end().to_owned());
        }
    }

    /// Every line kept, oldest first.
    pub fn lines(&self) -> Vec<String> {
        self.lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .cloned()
            .collect()
    }
}

impl std::fmt::Debug for LogSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogSink").finish_non_exhaustive()
    }
}

/// Where the lines go is not part of what a launch is: two specs that differ only in their
/// logs are the same launch.
impl PartialEq for LogSink {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for LogSink {}
