//! The walk that finds the project's files, on a thread of its own.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ignore::{DirEntry, WalkBuilder};
use tokio::sync::mpsc;

use crate::event::Event;

/// Files are sent in batches, so the finder has something to show quickly
/// without a message per file.
const BATCH_SIZE: usize = 500;
const BATCH_INTERVAL: Duration = Duration::from_millis(50);
/// The version control directory is never entered.
const HIDDEN_DIRECTORY: &str = ".git";

/// Keeps a walk going; dropping it stops the walk.
#[derive(Debug)]
pub(crate) struct ScanHandle {
    cancelled: Arc<AtomicBool>,
}

impl Drop for ScanHandle {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

/// Starts walking `root` and sends [`Event::FinderBatch`]es tagged with
/// `scan`, then [`Event::FinderDone`]. The files follow the explorer's rules:
/// what `.gitignore` and friends exclude is left out, other hidden files are
/// kept, `.git` is skipped.
pub(crate) fn start(root: PathBuf, scan: u64, events: mpsc::Sender<Event>) -> ScanHandle {
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancelled);
    std::thread::spawn(move || walk(&root, scan, &events, &flag));
    ScanHandle { cancelled }
}

fn walk(root: &Path, scan: u64, events: &mpsc::Sender<Event>, cancelled: &AtomicBool) {
    let walker = WalkBuilder::new(root)
        .hidden(false)
        // Honor .gitignore files even outside a git repository.
        .require_git(false)
        .filter_entry(|entry| entry.file_name() != HIDDEN_DIRECTORY)
        .build();

    let mut batch = Vec::new();
    let mut unreadable = 0;
    let mut last_sent = Instant::now();
    for entry in walker {
        if cancelled.load(Ordering::Relaxed) {
            return;
        }
        let Ok(entry) = entry else {
            unreadable += 1;
            continue;
        };
        if !is_file(&entry) {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        batch.push(relative.to_path_buf());
        if batch.len() >= BATCH_SIZE || last_sent.elapsed() >= BATCH_INTERVAL {
            let files = std::mem::take(&mut batch);
            if events
                .blocking_send(Event::FinderBatch { scan, files })
                .is_err()
            {
                return;
            }
            last_sent = Instant::now();
        }
    }
    if !batch.is_empty()
        && events
            .blocking_send(Event::FinderBatch { scan, files: batch })
            .is_err()
    {
        return;
    }
    // The receiver being gone means the app is quitting.
    let _ = events.blocking_send(Event::FinderDone { scan, unreadable });
}

/// A regular file, or a link to one.
fn is_file(entry: &DirEntry) -> bool {
    let Some(kind) = entry.file_type() else {
        return false;
    };
    kind.is_file() || (kind.is_symlink() && entry.path().is_file())
}
