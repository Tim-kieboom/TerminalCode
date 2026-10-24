//! The breakpoints a user set: on lines of files, kept as the text moves (a place is a byte,
//! moved through every edit, the way the places of the jump list are), remembered between runs
//! with a few characters of the line they were set on, and handed to a debug adapter as the
//! lines of each file.
//!
//! A breakpoint is in one of three states for the user:
//! - set (the usual);
//! - hollow: a debug adapter said it cannot be placed there (a comment, an empty line);
//! - moved: it was remembered from an earlier run, and the line it is on is not the text it was
//!   set on any more (the file changed outside the editor). It stays where it was set, dimmed,
//!   until the line is edited or the breakpoint is removed.
//!
//! See "Run configurations and a debugger" in `docs/design.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use trcode_ids::DocumentId;

/// How many characters of a line are kept to know it again.
const SNAPSHOT: usize = 80;

/// The text of a document, as far as the breakpoints need it.
pub trait Text {
    fn line_count(&self) -> usize;
    /// The byte where `line` starts.
    fn line_start(&self, line: usize) -> usize;
    /// The line the byte is on.
    fn line_of(&self, byte: usize) -> usize;
    /// The text of `line`, without its line break.
    fn line_text(&self, line: usize) -> String;
}

/// An edit of a document: the bytes `start..old_end` became `start..new_end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    pub start: usize,
    pub old_end: usize,
    pub new_end: usize,
}

/// What a debug adapter made of a breakpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub verified: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum Verification {
    #[default]
    Unknown,
    Verified,
    Unverified(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Mark {
    /// The line it is on, from 0.
    line: usize,
    /// The start of that line, while the document is open: it is what moves with the edits.
    byte: Option<usize>,
    /// The start of the text of the line when it was set (or last edited).
    text: String,
    stale: bool,
    verification: Verification,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct File {
    /// The open document of the file, when it is open.
    document: Option<DocumentId>,
    /// In order of line, one for each line.
    marks: Vec<Mark>,
    /// The lines last handed to the adapter, to tell what changed since.
    sent: Vec<usize>,
}

/// How a breakpoint is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Set,
    /// The adapter cannot place it there.
    Hollow,
    /// What it was set on is not on its line any more.
    Moved,
}

/// A breakpoint to draw: the line, from 0, and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    pub line: usize,
    pub kind: Kind,
}

/// A breakpoint as it is remembered between runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub path: PathBuf,
    pub line: usize,
    pub text: String,
}

/// All the breakpoints of the project, by the file they are in.
#[derive(Debug, Default)]
pub struct Breakpoints {
    files: BTreeMap<PathBuf, File>,
}

impl Breakpoints {
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How many breakpoints there are.
    pub fn len(&self) -> usize {
        self.files.values().map(|file| file.marks.len()).sum()
    }

    /// Sets a breakpoint on `line` of the file at `path` (open as `document`, with this `text`),
    /// or removes the one there. Returns whether one is set now.
    pub fn toggle(
        &mut self,
        path: &Path,
        document: DocumentId,
        text: &impl Text,
        line: usize,
    ) -> bool {
        let file = self.files.entry(path.to_path_buf()).or_default();
        file.document = Some(document);
        if let Some(at) = file.marks.iter().position(|mark| mark.line == line) {
            // A file left with none stays until the adapter has been told (`take_changes`).
            file.marks.remove(at);
            return false;
        }
        file.marks.push(Mark {
            line,
            byte: Some(text.line_start(line)),
            text: snapshot(&text.line_text(line)),
            stale: false,
            verification: Verification::Unknown,
        });
        file.marks.sort_by_key(|mark| mark.line);
        true
    }

    /// Takes away every breakpoint. Returns how many there were.
    pub fn clear(&mut self) -> usize {
        let count = self.len();
        for file in self.files.values_mut() {
            file.marks.clear();
        }
        count
    }

    /// The file at `path` is open as `document`: its breakpoints are put on its lines, and
    /// one whose line does not have the text it was set on is marked as moved (a line past the
    /// end of the file is too, and goes to the last line).
    pub fn attach(&mut self, path: &Path, document: DocumentId, text: &impl Text) {
        let Some(file) = self.files.get_mut(path) else {
            return;
        };
        file.document = Some(document);
        let count = text.line_count();
        for mark in &mut file.marks {
            let past_the_end = mark.line >= count;
            let line = mark.line.min(count.saturating_sub(1));
            let now = snapshot(&text.line_text(line));
            mark.stale = past_the_end || (!mark.text.is_empty() && mark.text != now);
            if !mark.stale {
                mark.text = now;
            }
            mark.line = line;
            mark.byte = Some(text.line_start(line));
        }
        file.settle();
    }

    /// The document is closed: its breakpoints stay on the lines they were last on.
    pub fn detach(&mut self, document: DocumentId) {
        for file in self.files.values_mut() {
            if file.document == Some(document) {
                file.document = None;
                for mark in &mut file.marks {
                    mark.byte = None;
                }
            }
        }
    }

    /// The path of the file that is open as `document`, if it has breakpoints.
    pub fn path_of(&self, document: DocumentId) -> Option<&Path> {
        self.files
            .iter()
            .find(|(_, file)| file.document == Some(document))
            .map(|(path, _)| path.as_path())
    }

    /// The document `document` was edited by `changes`, oldest first (`text` is what it is
    /// now): its breakpoints go with the text. One inside what was replaced is dropped; one
    /// on a line that was edited is no longer moved, and remembers what the line says now.
    pub fn remap(&mut self, document: DocumentId, changes: &[Change], text: &impl Text) {
        let Some(file) = self
            .files
            .values_mut()
            .find(|file| file.document == Some(document))
        else {
            return;
        };
        // The bytes follow the edits one by one; what each edit touched follows the ones after
        // it, so that it can be looked for in the text as it is now.
        let mut touched: Vec<(usize, usize)> = Vec::new();
        for change in changes {
            file.marks
                .retain_mut(|mark| shift_mark(mark, *change, changes.len() == 1, text));
            for (start, end) in &mut touched {
                *start = shifted(*start, *change);
                *end = shifted(*end, *change);
            }
            touched.push((change.start, change.new_end));
        }
        let length_lines = text.line_count().saturating_sub(1);
        for mark in &mut file.marks {
            if let Some(byte) = mark.byte {
                mark.line = text.line_of(byte).min(length_lines);
                // A line break taken away puts the byte inside a line: it goes to its start.
                mark.byte = Some(text.line_start(mark.line));
            }
            let edited = touched.iter().any(|(start, end)| {
                let first = text.line_of(*start).min(length_lines);
                let last = text.line_of(*end).min(length_lines);
                (first..=last).contains(&mark.line)
            });
            if edited {
                mark.stale = false;
                mark.text = snapshot(&text.line_text(mark.line));
            }
        }
        file.settle();
    }

    /// The documents that have breakpoints on them (open files).
    pub fn attached(&self) -> Vec<DocumentId> {
        self.files
            .values()
            .filter_map(|file| file.document)
            .collect()
    }

    /// The breakpoints of the open document, to draw.
    pub fn shown(&self, document: DocumentId) -> Vec<Shown> {
        let Some(file) = self
            .files
            .values()
            .find(|file| file.document == Some(document))
        else {
            return Vec::new();
        };
        file.marks
            .iter()
            .map(|mark| Shown {
                line: mark.line,
                kind: mark.kind(),
            })
            .collect()
    }

    /// The lines of the breakpoints of the file at `path`, from 0, in order.
    pub fn lines(&self, path: &Path) -> Vec<usize> {
        self.files
            .get(path)
            .map(|file| file.marks.iter().map(|mark| mark.line).collect())
            .unwrap_or_default()
    }

    /// The files that have breakpoints.
    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.files
            .iter()
            .filter(|(_, file)| !file.marks.is_empty())
            .map(|(path, _)| path.as_path())
    }

    /// The files whose breakpoints are not the ones last handed over by this method, with
    /// their lines (from 0; a file that has none any more has an empty list, so that the adapter
    /// is told). Calling it again gives nothing until something changes.
    pub fn take_changes(&mut self) -> Vec<(PathBuf, Vec<usize>)> {
        let mut changed = Vec::new();
        for (path, file) in &mut self.files {
            let lines: Vec<usize> = file.marks.iter().map(|mark| mark.line).collect();
            if lines != file.sent {
                file.sent = lines.clone();
                changed.push((path.clone(), lines));
            }
        }
        // A file with no breakpoint that the adapter knows is told so once.
        self.files
            .retain(|_, file| !file.marks.is_empty() || !file.sent.is_empty());
        changed
    }

    /// Tells the adapter's answer about the breakpoints of `path`, one for each line last
    /// handed over by [`Breakpoints::take_changes`] (or [`Breakpoints::forget_sent`]), in
    /// that order. Answers that do not match what is there now are not used.
    pub fn answered(&mut self, path: &Path, answers: &[Answer]) {
        let Some(file) = self.files.get_mut(path) else {
            return;
        };
        if answers.len() != file.marks.len() {
            return;
        }
        for (mark, answer) in file.marks.iter_mut().zip(answers) {
            mark.verification = match (answer.verified, &answer.message) {
                (true, _) => Verification::Verified,
                (false, message) => Verification::Unverified(message.clone()),
            };
        }
    }

    /// A session is over (or starts): nothing was handed to an adapter, nothing is verified.
    pub fn forget_sent(&mut self) {
        for file in self.files.values_mut() {
            file.sent.clear();
            for mark in &mut file.marks {
                mark.verification = Verification::Unknown;
            }
        }
    }

    /// Why the adapter cannot place the breakpoint on `line` of the file at `path`, if it
    /// said so.
    pub fn problem(&self, path: &Path, line: usize) -> Option<&str> {
        let mark = self
            .files
            .get(path)?
            .marks
            .iter()
            .find(|mark| mark.line == line)?;
        match &mark.verification {
            Verification::Unverified(Some(message)) => Some(message.as_str()),
            _ => None,
        }
    }

    /// The breakpoints as they are remembered between runs.
    pub fn export(&self) -> Vec<Saved> {
        self.files
            .iter()
            .flat_map(|(path, file)| {
                file.marks.iter().map(|mark| Saved {
                    path: path.clone(),
                    line: mark.line,
                    text: mark.text.clone(),
                })
            })
            .collect()
    }

    /// Takes in what was remembered. A line that has a breakpoint already is left alone.
    pub fn import(&mut self, saved: impl IntoIterator<Item = Saved>) {
        for item in saved {
            let file = self.files.entry(item.path).or_default();
            if file.marks.iter().all(|mark| mark.line != item.line) {
                file.marks.push(Mark {
                    line: item.line,
                    byte: None,
                    text: item.text,
                    stale: false,
                    verification: Verification::Unknown,
                });
                file.marks.sort_by_key(|mark| mark.line);
            }
        }
    }
}

impl File {
    /// Puts the marks in order of line, one for each line.
    fn settle(&mut self) {
        self.marks.sort_by_key(|mark| mark.line);
        self.marks.dedup_by_key(|mark| mark.line);
    }
}

impl Mark {
    fn kind(&self) -> Kind {
        if self.stale {
            Kind::Moved
        } else if matches!(self.verification, Verification::Unverified(_)) {
            Kind::Hollow
        } else {
            Kind::Set
        }
    }
}

/// Moves a breakpoint through one edit. `false` when the edit replaced the start of its line.
/// `alone`: the edit is the only one of the batch, which `text` is the result of.
fn shift_mark(mark: &mut Mark, change: Change, alone: bool, text: &impl Text) -> bool {
    let Some(byte) = mark.byte else {
        return true;
    };
    if change.start < byte && byte < change.old_end {
        return false;
    }
    // A line break typed at the start of the line takes the code down, and the breakpoint with
    // it.
    let code_moved_down = alone
        && byte == change.start
        && change.old_end == change.start
        && change.new_end > change.start
        && text.line_start(text.line_of(change.new_end)) == change.new_end;
    mark.byte = Some(match code_moved_down {
        true => change.new_end,
        false => shifted(byte, change),
    });
    true
}

/// The start of a line, without the blanks around it, to know it again.
fn snapshot(line: &str) -> String {
    line.trim().chars().take(SNAPSHOT).collect()
}

/// Where a byte ends up after `change`: text typed at it goes after it.
fn shifted(byte: usize, change: Change) -> usize {
    if byte <= change.start {
        byte
    } else if byte >= change.old_end {
        byte - change.old_end + change.new_end
    } else {
        change.start
    }
}

#[cfg(test)]
mod tests;
