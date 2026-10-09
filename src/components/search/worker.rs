//! The search itself, on a thread of its own.

use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use grep_matcher::{LineTerminator, Matcher};
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkMatch};
use ignore::{DirEntry, WalkBuilder};
use thiserror::Error;
use tokio::sync::mpsc;

use super::{Hit, Options};
use crate::event::Event;

/// Results are cut off here; a search for `e` would otherwise never end.
pub(crate) const MAX_HITS: usize = 5000;
/// One file, a log say, cannot use up the budget alone.
const MAX_PER_FILE: usize = 200;
const BATCH_HITS: usize = 100;
const BATCH_INTERVAL: Duration = Duration::from_millis(50);
/// A search waits this long before it starts, and does not start if a newer
/// one replaced it meanwhile, so typing a word searches for the word only.
const DEBOUNCE: Duration = Duration::from_millis(150);
const HIDDEN_DIRECTORY: &str = ".git";

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("{0}")]
    Pattern(String),
}

/// Keeps a search going; dropping it stops the search.
#[derive(Debug)]
pub(crate) struct SearchHandle {
    cancelled: Arc<AtomicBool>,
}

impl SearchHandle {
    /// A handle that is not tied to any search.
    #[cfg(test)]
    pub(crate) fn inert() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Drop for SearchHandle {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

/// Starts searching the files under `root` for `query` and sends
/// [`Event::SearchBatch`]es tagged with `id`, then [`Event::SearchDone`].
/// The files are the ones the explorer shows: ignored files are skipped,
/// binary files too. Fails, without starting anything, if the query is not a
/// valid pattern.
pub(crate) fn start(
    root: PathBuf,
    id: u64,
    query: &str,
    options: Options,
    events: mpsc::Sender<Event>,
) -> Result<SearchHandle, SearchError> {
    let matcher = RegexMatcherBuilder::new()
        .case_insensitive(!options.case_sensitive)
        .fixed_strings(!options.regex)
        .build(query)
        .map_err(|error| SearchError::Pattern(short_message(&error.to_string())))?;

    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancelled);
    std::thread::spawn(move || {
        std::thread::sleep(DEBOUNCE);
        if !flag.load(Ordering::Relaxed) {
            run(&root, id, &matcher, &events, &flag);
        }
    });
    Ok(SearchHandle { cancelled })
}

/// The part of a pattern error worth showing on one line.
fn short_message(message: &str) -> String {
    message
        .lines()
        .find_map(|line| line.trim().strip_prefix("error: "))
        .or_else(|| message.lines().next())
        .unwrap_or(message)
        .to_owned()
}

fn run(
    root: &Path,
    id: u64,
    matcher: &RegexMatcher,
    events: &mpsc::Sender<Event>,
    cancelled: &AtomicBool,
) {
    let walker = WalkBuilder::new(root)
        .hidden(false)
        // Honor .gitignore files even outside a git repository.
        .require_git(false)
        .sort_by_file_path(|a, b| a.cmp(b))
        .filter_entry(|entry| entry.file_name() != HIDDEN_DIRECTORY)
        .build();
    let mut lf_searcher = searcher(LineTerminator::byte(b'\n'));
    let mut crlf_searcher = searcher(LineTerminator::crlf());

    let mut batch = Vec::new();
    let mut total = 0;
    let mut files = 0;
    let mut truncated = false;
    let mut last_sent = Instant::now();

    for entry in walker {
        if cancelled.load(Ordering::Relaxed) {
            return;
        }
        let Ok(entry) = entry else { continue };
        if !is_file(&entry) {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };

        let before = batch.len();
        let mut sink = HitSink {
            matcher,
            path: relative,
            hits: &mut batch,
            found: 0,
            cancelled,
        };
        // A file that cannot be read is skipped, like one that is ignored.
        // The terminator decides what `$` matches before: `\n` alone would leave
        // the `\r` of a Windows line ending in the way.
        let searcher = match has_crlf_lines(entry.path()) {
            true => &mut crlf_searcher,
            false => &mut lf_searcher,
        };
        let _ = searcher.search_path(matcher, entry.path(), &mut sink);
        if batch.len() > before {
            files += 1;
            total += batch.len() - before;
        }

        let due = batch.len() >= BATCH_HITS || last_sent.elapsed() >= BATCH_INTERVAL;
        if due && !batch.is_empty() {
            let hits = std::mem::take(&mut batch);
            if events
                .blocking_send(Event::SearchBatch { search: id, hits })
                .is_err()
            {
                return;
            }
            last_sent = Instant::now();
        }
        if total >= MAX_HITS {
            truncated = true;
            break;
        }
    }

    if !batch.is_empty() {
        let hits = std::mem::take(&mut batch);
        if events
            .blocking_send(Event::SearchBatch { search: id, hits })
            .is_err()
        {
            return;
        }
    }
    // The receiver being gone means the app is quitting.
    let _ = events.blocking_send(Event::SearchDone {
        search: id,
        files,
        truncated,
    });
}

fn searcher(terminator: LineTerminator) -> Searcher {
    SearcherBuilder::new()
        .line_terminator(terminator)
        .line_number(true)
        .binary_detection(BinaryDetection::quit(b'\x00'))
        .build()
}

/// Whether the file's lines end in `\r\n`, judged by its first line break in
/// the first few kilobytes. A file that mixes both is searched by its first.
fn has_crlf_lines(path: &Path) -> bool {
    let mut head = [0u8; 8192];
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let Ok(read) = file.read(&mut head) else {
        return false;
    };
    head[..read]
        .iter()
        .position(|byte| *byte == b'\n')
        .is_some_and(|newline| newline > 0 && head[newline - 1] == b'\r')
}

/// A regular file, or a link to one.
fn is_file(entry: &DirEntry) -> bool {
    let Some(kind) = entry.file_type() else {
        return false;
    };
    kind.is_file() || (kind.is_symlink() && entry.path().is_file())
}

/// `line` without a trailing `\n` or `\r\n`.
fn trim_line_break(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.strip_suffix(b"\r").unwrap_or(line)
}

/// Collects the first match of each matching line of one file.
struct HitSink<'a> {
    matcher: &'a RegexMatcher,
    path: &'a Path,
    hits: &'a mut Vec<Hit>,
    found: usize,
    cancelled: &'a AtomicBool,
}

impl Sink for HitSink<'_> {
    type Error = io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, io::Error> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Ok(false);
        }
        // Without its line break, so that `$` matches at the end of the line.
        let bytes = trim_line_break(mat.bytes());
        let Ok(Some(found)) = self.matcher.find(bytes) else {
            return Ok(true);
        };

        // Offsets are into the bytes of the file; the line is shown as text
        // with invalid UTF-8 replaced, which can change the lengths.
        let line = String::from_utf8_lossy(bytes);
        let start = String::from_utf8_lossy(&bytes[..found.start()]).len();
        let len = String::from_utf8_lossy(&bytes[found.start()..found.end()]).len();
        let line_number = mat.line_number().unwrap_or(1);
        self.hits.push(Hit::new(
            self.path.to_path_buf(),
            line_number,
            &line,
            start..start + len,
        ));

        self.found += 1;
        Ok(self.found < MAX_PER_FILE)
    }
}
