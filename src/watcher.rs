//! Watching directories for changes made outside the editor.
//!
//! Directories are watched one by one, not recursively: the ones the explorer
//! has open and the ones that hold open files. A recursive watch of a project
//! would also cover `target/` or `node_modules/`, which the explorer hides
//! and which can exhaust the operating system's watch limit.

use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use notify_debouncer_mini::notify::{self, RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};
use thiserror::Error;
use tokio::sync::mpsc;

use crate::event::Event;

/// Changes closer together than this arrive as one event. Saving a file
/// usually touches several paths (a temporary file, then the rename).
const DEBOUNCE: Duration = Duration::from_millis(150);

#[derive(Debug, Error)]
pub enum WatchError {
    #[error("cannot start the file watcher: {0}")]
    Start(notify::Error),
    #[error("cannot watch {}: {source}", path.display())]
    Watch {
        path: PathBuf,
        source: notify::Error,
    },
}

/// Delivers [`Event::FilesChanged`] to the app loop for the directories it is
/// told to watch.
pub(crate) struct FsWatcher {
    debouncer: Debouncer<RecommendedWatcher>,
    watched: BTreeSet<PathBuf>,
}

impl fmt::Debug for FsWatcher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FsWatcher")
            .field("watched", &self.watched)
            .finish_non_exhaustive()
    }
}

impl FsWatcher {
    pub(crate) fn new(events: mpsc::Sender<Event>) -> Result<Self, WatchError> {
        let debouncer = new_debouncer(DEBOUNCE, move |result: DebounceEventResult| {
            let event = match result {
                Ok(changes) => {
                    let paths: Vec<PathBuf> =
                        changes.into_iter().map(|change| change.path).collect();
                    if paths.is_empty() {
                        return;
                    }
                    Event::FilesChanged(paths)
                }
                Err(error) => Event::WatchFailed(error.to_string()),
            };
            // This runs on the watcher's own thread, so blocking is fine. A
            // closed channel means the app is shutting down.
            let _ = events.blocking_send(event);
        })
        .map_err(WatchError::Start)?;

        Ok(Self {
            debouncer,
            watched: BTreeSet::new(),
        })
    }

    /// Watches exactly the directories in `wanted`: starts on the new ones and
    /// stops on the ones no longer wanted. Directories that cannot be watched
    /// are skipped (and tried again next time); the first such error is
    /// returned.
    pub(crate) fn watch_only(&mut self, wanted: &BTreeSet<PathBuf>) -> Result<(), WatchError> {
        let watcher = self.debouncer.watcher();

        let dropped: Vec<PathBuf> = self.watched.difference(wanted).cloned().collect();
        for path in dropped {
            // Best effort: the directory may be gone already.
            let _ = watcher.unwatch(&path);
            self.watched.remove(&path);
        }

        let mut first_error = None;
        for path in wanted {
            if self.watched.contains(path) {
                continue;
            }
            match watcher.watch(path, RecursiveMode::NonRecursive) {
                Ok(()) => {
                    self.watched.insert(path.clone());
                }
                Err(source) => {
                    first_error.get_or_insert(WatchError::Watch {
                        path: path.clone(),
                        source,
                    });
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    #[cfg(test)]
    pub(crate) fn watched(&self) -> &BTreeSet<PathBuf> {
        &self.watched
    }
}
