//! The thread that parses. Documents hand it jobs; it keeps the tree of every
//! document it has seen, so a job only has to say what changed, and sends
//! back spans. Jobs for the same document are coalesced: while the worker is
//! busy only the newest one is kept (its edits joined to the ones it
//! replaces), so a typist is never behind by more than one parse.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;

use ropey::Rope;

use super::{Highlighter, Language, Span};
use crate::buffer::EditInfo;
use crate::ui::theme::Theme;

/// Tells jobs of different documents apart.
pub(crate) type Key = u64;

/// What the worker is asked to do for one document.
#[derive(Debug, Clone)]
pub(crate) struct Job {
    pub(crate) key: Key,
    pub(crate) language: Language,
    /// The text, a cheap copy of the buffer's rope.
    pub(crate) text: Rope,
    /// The buffer version of `text`.
    pub(crate) version: u64,
    /// The version the previous job for this document had, if there was one,
    /// and the edits that lead from it to `version`.
    pub(crate) base: Option<u64>,
    pub(crate) edits: Vec<EditInfo>,
    /// The byte ranges to produce spans for.
    pub(crate) ranges: Vec<Range<usize>>,
}

/// How the worker got its tree for a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Parse {
    /// The text was         && entry.parsed.and_then(|parsed| job.version.checked_sub(parsed)) == Some(job.edits.len() as u64) from scratch.
    Full,
    /// The old tree was updated with the edits and reparsed.
    Incremental,
    /// The tree was already right for this version; only the query ran.
    Reused,
}

/// What the worker found, for the app loop to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Output {
    pub(crate) key: Key,
    /// The buffer version the spans belong to.
    pub(crate) version: u64,
    pub(crate) spans: Vec<Span>,
    pub(crate) parse: Parse,
    /// Set when the document could not be highlighted at all.
    pub(crate) failure: Option<String>,
}

/// Starts parsing on its own thread and sends every result to `deliver`.
/// Dropping the worker stops the thread once its current job is done.
pub(crate) struct SyntaxWorker {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for SyntaxWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyntaxWorker").finish_non_exhaustive()
    }
}

#[derive(Default)]
struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
}

#[derive(Default)]
struct Queue {
    /// At most one job per document: the newest.
    jobs: HashMap<Key, Job>,
    /// Documents that are gone, so their trees can be dropped.
    forgotten: Vec<Key>,
    closed: bool,
}

impl SyntaxWorker {
    pub(crate) fn spawn(theme: Theme, deliver: impl Fn(Output) + Send + 'static) -> Self {
        let shared = Arc::new(Shared::default());
        let worker = Arc::clone(&shared);
        thread::spawn(move || run(&worker, &theme, &deliver));
        Self { shared }
    }

    /// Asks for spans. A job for the same document that has not started yet is
    /// replaced by this one, which takes over its edits.
    pub(crate) fn submit(&self, job: Job) {
        let mut queue = self.shared.lock();
        let job = match queue.jobs.remove(&job.key) {
            Some(older) => coalesce(older, job),
            None => job,
        };
        queue.jobs.insert(job.key, job);
        self.shared.wake.notify_one();
    }

    /// The document is gone; drop what is kept for it.
    pub(crate) fn forget(&self, key: Key) {
        let mut queue = self.shared.lock();
        queue.jobs.remove(&key);
        queue.forgotten.push(key);
        self.shared.wake.notify_one();
    }
}

impl Drop for SyntaxWorker {
    fn drop(&mut self) {
        self.shared.lock().closed = true;
        self.shared.wake.notify_one();
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Queue> {
        // A panic in the worker thread must not take the app down with it.
        self.queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// `newer` replaces `older`, which never started. If `newer` continues from
/// where `older` ends, their edits are joined and the result still continues
/// from where `older` began; otherwise `newer` stands alone.
pub(super) fn coalesce(older: Job, newer: Job) -> Job {
    if older.language != newer.language || newer.base != Some(older.version) {
        return Job {
            base: None,
            edits: Vec::new(),
            ..newer
        };
    }
    let mut edits = older.edits;
    edits.extend(newer.edits);
    Job {
        base: older.base,
        edits,
        ..newer
    }
}

/// What the worker keeps for one document.
struct Kept {
    highlighter: Highlighter,
    language: Language,
    /// The version the tree was parsed for.
    parsed: Option<u64>,
}

fn run(shared: &Shared, theme: &Theme, deliver: &impl Fn(Output)) {
    let mut kept: HashMap<Key, Kept> = HashMap::new();
    loop {
        let job = {
            let mut queue = shared.lock();
            loop {
                for key in queue.forgotten.drain(..) {
                    kept.remove(&key);
                }
                if queue.closed {
                    return;
                }
                if let Some(&key) = queue.jobs.keys().next()
                    && let Some(job) = queue.jobs.remove(&key)
                {
                    break job;
                }
                queue = shared
                    .wake
                    .wait(queue)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        };
        deliver(process(&mut kept, theme, job));
    }
}

fn process(kept: &mut HashMap<Key, Kept>, theme: &Theme, job: Job) -> Output {
    let key = job.key;
    let failed = |message: String| Output {
        key,
        version: job.version,
        spans: Vec::new(),
        parse: Parse::Full,
        failure: Some(message),
    };
    if kept.get(&key).is_none_or(|k| k.language != job.language) {
        match Highlighter::new(job.language, theme) {
            Ok(highlighter) => {
                kept.insert(
                    key,
                    Kept {
                        highlighter,
                        language: job.language,
                        parsed: None,
                    },
                );
            }
            Err(error) => return failed(error.to_string()),
        }
    }
    let Some(entry) = kept.get_mut(&key) else {
        return failed("the highlighter vanished".to_owned());
    };

    let parse = if entry.parsed == Some(job.version) {
        Parse::Reused
    } else if entry.parsed.is_some()
        && entry.parsed == job.base
        && entry.parsed.map(|parsed| job.version - parsed) == Some(job.edits.len() as u64)
    {
        entry.highlighter.edit(&job.edits);
        Parse::Incremental
    } else {
        Parse::Full
    };
    let parsed = match parse {
        Parse::Reused => Ok(()),
        Parse::Incremental => entry.highlighter.reparse(&job.text),
        Parse::Full => entry.highlighter.parse(&job.text),
    };
    if let Err(error) = parsed {
        entry.parsed = None;
        return failed(error.to_string());
    }
    entry.parsed = Some(job.version);

    let spans = job
        .ranges
        .iter()
        .flat_map(|range| entry.highlighter.spans(&job.text, range.clone()))
        .collect();
    Output {
        key,
        version: job.version,
        spans,
        parse,
        failure: None,
    }
}
