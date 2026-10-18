//! What to do when a plugin traps: start it again, unless it keeps doing it.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// How long a trap counts against a plugin.
const WINDOW: Duration = Duration::from_secs(60);
/// How many traps within the window disable a plugin.
const LIMIT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Verdict {
    /// Throw the instance away and start a fresh one.
    Restart,
    /// Too many traps: leave the plugin off.
    Disable,
}

/// The traps of one plugin that are still recent.
#[derive(Debug, Default)]
pub(super) struct Strikes {
    traps: VecDeque<Instant>,
}

impl Strikes {
    /// Notes a trap at `now` and says what to do. A quiet minute wipes the count.
    pub(super) fn record(&mut self, now: Instant) -> Verdict {
        while self
            .traps
            .front()
            .is_some_and(|trap| now.duration_since(*trap) >= WINDOW)
        {
            self.traps.pop_front();
        }
        self.traps.push_back(now);
        match self.traps.len() >= LIMIT {
            true => Verdict::Disable,
            false => Verdict::Restart,
        }
    }
}
