use std::ops::Range;

use super::mapping::map_spans;
#[cfg(test)]
use super::worker::Parse;
use super::worker::{Job, Key, Output, SyntaxWorker};
use super::{Language, Span};
use crate::buffer::{Buffer, EditInfo};

/// More edits than this are not kept: the worker parses afresh instead, which
/// is cheaper than replaying them anyway.
const MAX_LOGGED_EDITS: usize = 10_000;

/// A document's side of syntax highlighting: the spans on screen, kept right
/// while the worker parses, and what it takes to ask the worker for new ones.
///
/// The worker is slow compared to typing, so its answer is for text that has
/// since changed. The document therefore logs every edit made since the
/// version the spans describe, and moves the spans through them when an
/// answer arrives, and again whenever an edit comes in.
#[derive(Debug)]
pub(crate) struct DocumentSyntax {
    key: Key,
    /// The language the worker has been asked to parse this document as.
    language: Option<Language>,
    /// The language that could not be set up, so it is not retried every frame.
    failed: Option<Language>,
    error: Option<String>,
    /// The spans for the text as it is now (as far as the edits tell).
    spans: Vec<Span>,
    /// The version the oldest logged edit starts from, or `None` when the log
    /// does not lead to the buffer's version (nothing asked yet, or a change
    /// nobody reported).
    log_base: Option<u64>,
    /// The edits since `log_base`, in order.
    log: Vec<EditInfo>,
    /// The version and ranges the worker was last asked for.
    submitted: Option<(u64, Vec<Range<usize>>)>,
    /// Whether the newest question has not been answered yet.
    #[cfg(test)]
    awaiting: bool,
    #[cfg(test)]
    parses: Parses,
}

#[cfg(test)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Parses {
    pub(crate) full: usize,
    pub(crate) incremental: usize,
    pub(crate) reused: usize,
}

impl DocumentSyntax {
    pub(crate) fn new(key: Key) -> Self {
        Self {
            key,
            language: None,
            failed: None,
            error: None,
            spans: Vec::new(),
            log_base: None,
            log: Vec::new(),
            submitted: None,
            #[cfg(test)]
            awaiting: false,
            #[cfg(test)]
            parses: Parses::default(),
        }
    }

    pub(crate) fn key(&self) -> Key {
        self.key
    }

    /// Notes edits made to the document's text, oldest first. The spans move
    /// with them at once, so they never describe text that is gone.
    pub(crate) fn record_edits(&mut self, edits: &[EditInfo]) {
        if self.log_base.is_none() {
            return;
        }
        if self.log.len() + edits.len() > MAX_LOGGED_EDITS {
            self.restart();
            return;
        }
        for edit in edits {
            map_spans(&mut self.spans, edit);
        }
        self.log.extend_from_slice(edits);
    }

    /// Asks the worker for spans for `buffer` and the byte `ranges` to show,
    /// if what it was last asked for is not that. The language comes from the
    /// buffer's path, so a rename that changes the extension changes it here.
    pub(crate) fn update(
        &mut self,
        buffer: &Buffer,
        ranges: &[Range<usize>],
        worker: &SyntaxWorker,
    ) {
        let language = buffer.path().and_then(Language::from_path);
        if self.language != language {
            if self.language.is_some() {
                worker.forget(self.key);
            }
            self.restart();
            self.language = language;
            self.failed = None;
            self.error = None;
        }
        let Some(language) = language else {
            return;
        };
        if self.failed == Some(language) {
            return;
        }

        let version = buffer.version();
        let tracked = self
            .log_base
            .is_some_and(|base| base + self.log.len() as u64 == version);
        if !tracked {
            // A change nobody reported (a reload): what is shown is wrong.
            self.restart();
            self.log_base = Some(version);
        }
        let unchanged = self
            .submitted
            .as_ref()
            .is_some_and(|(sent, wanted)| *sent == version && wanted == ranges);
        if unchanged {
            return;
        }

        let (base, edits) = match (&self.submitted, self.log_base) {
            (Some((sent, _)), Some(log_base)) => {
                let from = (*sent - log_base) as usize;
                (Some(*sent), self.log[from..].to_vec())
            }
            _ => (None, Vec::new()),
        };
        worker.submit(Job {
            key: self.key,
            language,
            text: buffer.snapshot(),
            version,
            base,
            edits,
            ranges: ranges.to_vec(),
        });
        self.submitted = Some((version, ranges.to_vec()));
        #[cfg(test)]
        {
            self.awaiting = true;
        }
    }

    /// Takes the worker's answer. An answer for text older than the spans
    /// already shown, or for text this document no longer has a record of, is
    /// ignored; any other is moved forward through the edits made since the
    /// text it describes, and replaces the spans. Returns whether the screen
    /// needs to be drawn again.
    pub(crate) fn accept(&mut self, output: Output) -> bool {
        if let Some(message) = output.failure {
            self.failed = self.language;
            self.error = Some(message);
            self.restart();
            return true;
        }
        let Some(base) = self.log_base else {
            return false;
        };
        let version = output.version;
        let newest = base + self.log.len() as u64;
        if version < base || version > newest {
            return false;
        }
        let behind = (version - base) as usize;
        let mut spans = output.spans;
        for edit in &self.log[behind..] {
            map_spans(&mut spans, edit);
        }
        self.spans = spans;
        self.log.drain(..behind);
        self.log_base = Some(version);
        #[cfg(test)]
        {
            self.awaiting = self
                .submitted
                .as_ref()
                .is_some_and(|(sent, _)| *sent != version);
        }
        #[cfg(test)]
        match output.parse {
            Parse::Full => self.parses.full += 1,
            Parse::Incremental => self.parses.incremental += 1,
            Parse::Reused => self.parses.reused += 1,
        }
        true
    }

    /// Forgets everything about the text: the spans, the log and what the
    /// worker was asked. The next update asks for everything again.
    /// Forgets what the worker was told and what it answered, and any failure,
    /// so the next update asks a new worker for everything afresh.
    pub(crate) fn start_over(&mut self) {
        self.restart();
        self.failed = None;
        self.error = None;
    }

    fn restart(&mut self) {
        #[cfg(test)]
        {
            self.awaiting = false;
        }
        self.spans.clear();
        self.log.clear();
        self.log_base = None;
        self.submitted = None;
    }

    /// Why highlighting could not be set up, once; asking again gives `None`
    /// until it fails anew.
    pub(crate) fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    /// The spans for the ranges last answered, in order, not overlapping.
    pub(crate) fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// Whether the worker still owes an answer to the newest question.
    #[cfg(test)]
    pub(crate) fn is_waiting(&self) -> bool {
        self.awaiting
    }

    #[cfg(test)]
    pub(crate) fn parses(&self) -> Parses {
        self.parses
    }
}
