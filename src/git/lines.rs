//! Which lines of a document differ from the file in git's index, for the
//! marks in the gutter. The index text is read once in the background (only the
//! hash of each line is kept); the difference with the buffer is worked out
//! again when the text changes, and only for documents that are on screen.

use std::path::{Path, PathBuf};

use super::Change;
use super::diff::{Hunk, HunkKind, diff, hash_line};
use crate::buffer::Buffer;

/// What the index was asked for: the file, and how git said it differs from the
/// last commit then. When either changes the index text may have too.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Asked {
    path: PathBuf,
    change: Option<Change>,
}

/// The state of the marks of one document.
#[derive(Debug, Default)]
pub(crate) struct Lines {
    /// The hash of each line of the index text; `None` when there is nothing to
    /// compare with (not asked yet, a file git ignores, a binary file).
    base: Option<Vec<u64>>,
    /// Counts the texts that were taken as the base.
    base_number: u64,
    asked: Option<Asked>,
    /// Numbers the requests, so the answer of one that was replaced is ignored.
    requests: u64,
    hunks: Vec<Hunk>,
    /// The buffer version and base the hunks are for.
    worked_out_for: Option<(u64, u64)>,
}

impl Lines {
    /// Whether the index text should be asked for again: nothing was asked for
    /// this file in this state yet.
    pub(crate) fn wants(&self, path: &Path, change: Option<Change>) -> bool {
        match &self.asked {
            Some(asked) => asked.path != path || asked.change != change,
            None => true,
        }
    }

    /// Notes that the index text of `path` is being asked for; returns the
    /// number its answer must come back with.
    pub(crate) fn ask(&mut self, path: PathBuf, change: Option<Change>) -> u64 {
        self.asked = Some(Asked { path, change });
        self.requests += 1;
        self.requests
    }

    /// Takes the answer to request `request` as the text to compare with:
    /// `None` is nothing to compare with, and no marks; an empty list is a file
    /// git does not have yet, all of whose lines are new. An answer to a request
    /// that was replaced is dropped. Returns whether the marks may differ now.
    pub(crate) fn accept(&mut self, request: u64, base: Option<Vec<u64>>) -> bool {
        if request != self.requests {
            return false;
        }
        self.base = base;
        self.base_number += 1;
        true
    }

    /// Notes that request `request` failed, so the same question is asked again
    /// at the next change; the marks stay as they are.
    pub(crate) fn failed(&mut self, request: u64) {
        if request == self.requests {
            self.asked = None;
        }
    }

    /// Forgets the index text, and so the marks.
    pub(crate) fn clear(&mut self) {
        *self = Self {
            // An answer that is still on its way is for the file as it was.
            requests: self.requests + 1,
            ..Self::default()
        };
    }

    /// Works out the marks again if the text or the base changed since.
    pub(crate) fn refresh(&mut self, buffer: &Buffer) {
        let key = (buffer.version(), self.base_number);
        if self.worked_out_for == Some(key) {
            return;
        }
        self.worked_out_for = Some(key);
        self.hunks = match &self.base {
            Some(base) => diff(base, &line_hashes(buffer)),
            None => Vec::new(),
        };
    }

    /// What happened to `line` (counted from 0), if anything.
    pub(crate) fn mark_at(&self, line: usize) -> Option<HunkKind> {
        let after = self.hunks.partition_point(|hunk| hunk.lines.start <= line);
        let hunk = self.hunks.get(after.checked_sub(1)?)?;
        // A removal is the empty range at the line after the removed ones.
        (hunk.lines.contains(&line) || hunk.lines.start == line).then_some(hunk.kind)
    }
}

fn line_hashes(buffer: &Buffer) -> Vec<u64> {
    (0..buffer.len_lines())
        .map(|line| match buffer.line_content(line) {
            Ok(content) => hash_line(&content),
            Err(_) => hash_line(""),
        })
        .collect()
}
