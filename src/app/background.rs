//! Work that runs on other threads and reports back through the event
//! channel (the file finder's walk, the project search).

use tokio::sync::mpsc;

use crate::event::Event;

/// Where background work sends its results, and the numbers that tell one run
/// from the one before it, so the results of a run that was replaced are
/// ignored.
#[derive(Debug, Default)]
pub(super) struct Background {
    /// Without it nothing can run in the background, and the finder and the
    /// search have no way to get their results.
    events: Option<mpsc::Sender<Event>>,
    scans_started: u64,
    searches_started: u64,
}

impl Background {
    pub(super) fn connect(&mut self, events: mpsc::Sender<Event>) {
        self.events = Some(events);
    }

    pub(super) fn is_connected(&self) -> bool {
        self.events.is_some()
    }

    /// A way to send results to the app loop, if there is one.
    pub(super) fn sender(&self) -> Option<mpsc::Sender<Event>> {
        self.events.clone()
    }

    /// The number for a new file finder walk. Numbers start at 1.
    pub(super) fn next_scan(&mut self) -> u64 {
        self.scans_started += 1;
        self.scans_started
    }

    /// The number for a new project search. Numbers start at 1.
    pub(super) fn next_search(&mut self) -> u64 {
        self.searches_started += 1;
        self.searches_started
    }
}

#[cfg(test)]
mod tests;
