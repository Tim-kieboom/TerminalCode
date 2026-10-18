//! When to ask git again. A refresh is asked for often (every change a watcher
//! reports) and costs a process, so this keeps one in flight, waits a moment
//! for a burst of changes to end, and waits twice as long as the last refresh
//! took before the next one (a slow repository is asked less, never switched
//! off). It knows nothing of git or threads, only of time.

use std::time::{Duration, Instant};

/// How long after the first change of a burst the refresh starts.
const SETTLE: Duration = Duration::from_millis(300);

#[derive(Debug, Default)]
pub(crate) struct Schedule {
    /// Something changed since the refresh that started last.
    wanted: bool,
    /// A refresh is running.
    running: bool,
    /// Not before this, whatever changed: twice as long as the last refresh took.
    backoff_until: Option<Instant>,
    /// When a wanted refresh may start.
    ready_at: Option<Instant>,
}

impl Schedule {
    /// Says that what git shows may have changed. Later calls while a refresh
    /// is already wanted do not move it back, so a steady stream of changes
    /// cannot keep it from ever running.
    pub(crate) fn request(&mut self, now: Instant) {
        if self.wanted {
            return;
        }
        self.wanted = true;
        self.ready_at = Some(self.not_before(now + SETTLE));
    }

    /// Says that what git shows changed because of something the editor just
    /// did: no wait for a burst to end, only for the back-off.
    pub(crate) fn request_now(&mut self, now: Instant) {
        let soon = self.not_before(now);
        self.wanted = true;
        self.ready_at = Some(self.ready_at.map_or(soon, |at| at.min(soon)));
    }

    /// `at`, or the end of the back-off when that is later.
    fn not_before(&self, at: Instant) -> Instant {
        self.backoff_until.map_or(at, |until| until.max(at))
    }

    /// When the loop should look again at this: when a wanted refresh may
    /// start. `None` when none is wanted or one is running.
    pub(crate) fn deadline(&self) -> Option<Instant> {
        (self.wanted && !self.running).then_some(self.ready_at?)
    }

    /// Whether a refresh should start now.
    pub(crate) fn is_due(&self, now: Instant) -> bool {
        self.deadline().is_some_and(|at| now >= at)
    }

    /// Notes that the refresh started.
    pub(crate) fn started(&mut self) {
        self.wanted = false;
        self.running = true;
    }

    /// Notes that the refresh ended after `took`; the next waits twice that.
    pub(crate) fn finished(&mut self, now: Instant, took: Duration) {
        self.running = false;
        let until = now + took * 2;
        self.backoff_until = Some(until);
        // A change that came while it ran waits for the back-off too.
        self.ready_at = self.ready_at.map(|at| at.max(until));
    }
}
