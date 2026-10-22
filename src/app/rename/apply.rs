//! Working out, and doing, the edit a rename or a code action answers with. All or nothing: every
//! file is checked before anything is written, the files on disk are written to
//! temporary files before any is put in its place, and the open documents are
//! edited last. The one thing that can still go half way is putting the
//! temporary files in place, and then the error says which files were changed.
//!
//! Open documents and files that are not open go through the same code: a file
//! that is not open is read into a rope, the server's positions are turned into
//! bytes of the rope, and the edits are made on it.

use std::collections::HashMap;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};

use ropey::Rope;
use thiserror::Error;

use crate::buffer::{FileError, StagedWrite};
use crate::files::paths::canonical;
use crate::ids::DocumentId;
use crate::lsp::uri::uri_to_path;
use crate::lsp::{RenameEdit, WorkspaceEdit, from_lsp};

/// Why an edit was not made. Unless it says otherwise, nothing was changed.
#[derive(Debug, Error)]
pub(in crate::app) enum EditError {
    #[error(
        "the language server wants to {0}, which this version cannot do (only text edits and renames of files are applied); nothing was changed"
    )]
    Operation(String),
    #[error(
        "the language server wants to move {0}, which is outside the project; nothing was changed"
    )]
    MoveOutside(String),
    #[error("{0} is not there (any more) to be moved; nothing was changed")]
    MoveMissing(String),
    #[error("{0} is there already, and an edit does not replace a file; nothing was changed")]
    MoveExists(String),
    #[error("the language server wants to move {0} into itself; nothing was changed")]
    MoveIntoItself(String),
    #[error(
        "the edit was cut short: moved {done}; could not move {failed}: {source}; the text of the files was changed already"
    )]
    MoveFailed {
        done: String,
        failed: String,
        source: std::io::Error,
    },
    #[error("the language server named {0}, which is not a file; nothing was changed")]
    NotAFile(String),
    #[error(
        "{0} is outside the project, and an edit only changes files inside it; nothing was changed"
    )]
    Outside(String),
    #[error("{0} is not there (any more); nothing was changed")]
    Missing(String),
    #[error("{0} is not valid UTF-8, so it cannot be edited; nothing was changed")]
    NotUtf8(String),
    #[error("{0} is read-only, so it cannot be edited; nothing was changed")]
    ReadOnly(String),
    #[error("{0} changed while the language server was working; nothing was changed, try again")]
    Changed(String),
    #[error(
        "the language server's positions do not fit {0} (it may have changed on disk since the server read it); nothing was changed"
    )]
    Stale(String),
    #[error("the language server sent edits that overlap in {0}; nothing was changed")]
    Overlap(String),
    #[error("could not read {name}: {source}; nothing was changed")]
    Read {
        name: String,
        source: std::io::Error,
    },
    #[error("could not write {name}: {source}; nothing was changed")]
    Write { name: String, source: FileError },
    #[error("{0} is not open, so the edit cannot be made as an undo step; nothing was changed")]
    NotOpen(String),
    #[error(
        "the edit was cut short: changed {done}; could not write {failed}: {source}; the other files, and the open ones, were not changed"
    )]
    Torn {
        done: String,
        failed: String,
        source: FileError,
    },
}

/// What the edit is checked against.
#[derive(Debug, Clone, Copy)]
pub(in crate::app) enum Check<'a> {
    /// A rename of this name: the text at every range has to be it, and files
    /// that are not open are written.
    Rename(&'a str),
    /// A code action: its edits have no old text to compare (an import is an
    /// insertion), so only open documents are edited, by the version they had
    /// when the server was asked.
    Action,
    /// A format: like an action, and a file anywhere that can be saved is
    /// formatted, not only one under the project root.
    Format,
}

/// An open document, as it is now.
pub(in crate::app) struct OpenDocument {
    pub(in crate::app) id: DocumentId,
    pub(in crate::app) version: u64,
    pub(in crate::app) text: Rope,
    pub(in crate::app) read_only: bool,
}

/// What a rename was checked to do.
#[derive(Debug)]
pub(in crate::app) struct Plan {
    /// Files that are not open, with the text they get.
    closed: Vec<Closed>,
    /// Open documents, with the replacements for each, in text order.
    open: Vec<Open>,
    /// Files and folders that move, in the order the server gave them, after the files are
    /// written and before the open documents are edited.
    moves: Vec<Move>,
    places: usize,
}

/// Where a move is: about to be made, or made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum MoveStage {
    Before,
    After,
}

/// The files an edit changes, where they are now (the edit may name a file by where it will
/// be once the edit's moves are made).
pub(in crate::app) fn files_now(edit: &WorkspaceEdit, root: &Path) -> Vec<PathBuf> {
    let root = canonical(root);
    let moves = moves_of(edit, &root).unwrap_or_default();
    edit.files
        .iter()
        .filter_map(|file| uri_to_path(&file.uri))
        .map(|path| source_of(&path, &moves))
        .collect()
}

/// A file or folder that moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) struct Move {
    pub(in crate::app) from: PathBuf,
    pub(in crate::app) to: PathBuf,
    /// How they are named in a message.
    from_name: String,
    to_name: String,
}

#[derive(Debug)]
struct Closed {
    path: PathBuf,
    name: String,
    text: String,
}

#[derive(Debug)]
pub(in crate::app) struct Open {
    pub(in crate::app) document: DocumentId,
    pub(in crate::app) replacements: Vec<(Range<usize>, String)>,
}

impl Plan {
    /// How many places change.
    pub(in crate::app) fn places(&self) -> usize {
        self.places
    }

    /// How many files change.
    pub(in crate::app) fn files(&self) -> usize {
        self.closed.len() + self.open.len()
    }

    /// The files and folders that move.
    pub(in crate::app) fn moves(&self) -> &[Move] {
        &self.moves
    }

    /// The open documents that get edits.
    pub(in crate::app) fn open(&self) -> &[Open] {
        &self.open
    }
}

/// Checks `edit` against the files and the open documents. `sent` holds the versions of the open documents when the server
/// was asked; `open` finds the open document of a path.
pub(in crate::app) fn plan(
    edit: &WorkspaceEdit,
    check: Check,
    root: &Path,
    sent: &HashMap<DocumentId, u64>,
    open: impl Fn(&Path) -> Option<OpenDocument>,
) -> Result<Plan, EditError> {
    if let Some(operation) = edit.operations.first() {
        return Err(EditError::Operation(operation.clone()));
    }
    let root = canonical(root);
    let moves = match (check, edit.renames.first()) {
        (Check::Format, Some(rename)) => {
            return Err(EditError::Operation(format!(
                "rename {} to {}",
                rename.old_uri, rename.new_uri
            )));
        }
        _ => moves_of(edit, &root)?,
    };
    let mut plan = Plan {
        closed: Vec::new(),
        open: Vec::new(),
        moves,
        places: 0,
    };
    for file in &edit.files {
        let named = uri_to_path(&file.uri).ok_or_else(|| EditError::NotAFile(file.uri.clone()))?;
        // An edit may name a file by where it will be after the moves: its text is where it is now.
        let path = source_of(&named, &plan.moves);
        let name = shown(&root, &path);
        if !matches!(check, Check::Format) && !canonical(&path).starts_with(&root) {
            return Err(EditError::Outside(name));
        }
        match open(&path) {
            Some(document) => {
                if sent.get(&document.id) != Some(&document.version) {
                    return Err(EditError::Changed(name));
                }
                if document.read_only {
                    return Err(EditError::ReadOnly(name));
                }
                let replacements = resolve(&document.text, &file.edits, check, &name)?;
                plan.places += replacements.len();
                plan.open.push(Open {
                    document: document.id,
                    replacements,
                });
            }
            None => {
                if matches!(check, Check::Action | Check::Format) {
                    return Err(EditError::NotOpen(name));
                }
                let text = read_closed(&path, &name)?;
                let replacements = resolve(&text, &file.edits, check, &name)?;
                plan.places += replacements.len();
                plan.closed.push(Closed {
                    path,
                    name,
                    text: edited(text, &replacements),
                });
            }
        }
    }
    Ok(plan)
}

/// The moves an edit asks for, checked: both ends inside the project, the thing there to
/// be moved, nothing at the new name (a rename the server says to skip when there is
/// something is left out), and not into itself.
fn moves_of(edit: &WorkspaceEdit, root: &Path) -> Result<Vec<Move>, EditError> {
    let mut moves: Vec<Move> = Vec::new();
    for rename in &edit.renames {
        let from = uri_to_path(&rename.old_uri)
            .ok_or_else(|| EditError::NotAFile(rename.old_uri.clone()))?;
        let to = uri_to_path(&rename.new_uri)
            .ok_or_else(|| EditError::NotAFile(rename.new_uri.clone()))?;
        let (from_name, to_name) = (shown(root, &from), shown(root, &to));
        for end in [&from, &to] {
            if !lenient(end).starts_with(root) {
                return Err(EditError::MoveOutside(shown(root, end)));
            }
        }
        // What an earlier move puts there is not on disk yet, and is a place to move from.
        let made_by_a_move = moves.iter().any(|done| from.starts_with(&done.to));
        if !made_by_a_move && !from.exists() {
            return Err(EditError::MoveMissing(from_name));
        }
        if canonical(&to).starts_with(canonical(&from)) && canonical(&to) != canonical(&from) {
            return Err(EditError::MoveIntoItself(from_name));
        }
        if to.exists() {
            if rename.ignore_if_exists {
                continue;
            }
            return Err(EditError::MoveExists(to_name));
        }
        moves.push(Move {
            from,
            to,
            from_name,
            to_name,
        });
    }
    Ok(moves)
}

/// `path` as it would be if `from` had been moved to `to`; `None` when it is not `from` or inside it.
fn rebase(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    let rest = path.strip_prefix(from).ok()?;
    Some(match rest.as_os_str().is_empty() {
        true => to.to_path_buf(),
        false => to.join(rest),
    })
}

/// Where the file an edit names is now: a path inside a place the edit moves to is where
/// it was before, the last move first.
fn source_of(path: &Path, moves: &[Move]) -> PathBuf {
    let mut path = path.to_path_buf();
    for done in moves.iter().rev() {
        if let Some(before) = rebase(&path, &done.to, &done.from) {
            path = before;
        }
    }
    path
}

/// `path` with the part that exists made canonical, so that a place that is not there yet can
/// be told to be inside the project or not.
fn lenient(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut here = path.to_path_buf();
    while !here.exists() {
        match (here.file_name().map(ToOwned::to_owned), here.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                here = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
    }
    let mut resolved = canonical(&here);
    for name in rest.into_iter().rev() {
        resolved.push(name);
    }
    resolved
}

/// The text of a file that is not open.
fn read_closed(path: &Path, name: &str) -> Result<Rope, EditError> {
    let read = |source| EditError::Read {
        name: name.to_owned(),
        source,
    };
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Err(EditError::Missing(name.to_owned()));
        }
        Err(source) => return Err(read(source)),
    };
    if metadata.permissions().readonly() {
        return Err(EditError::ReadOnly(name.to_owned()));
    }
    let bytes = fs::read(path).map_err(read)?;
    let text = String::from_utf8(bytes).map_err(|_| EditError::NotUtf8(name.to_owned()))?;
    Ok(Rope::from_str(&text))
}

/// The edits of one file as byte ranges of `text`, in text order. For a
/// rename each has to replace the old name (a raw identifier, `r#name`, counts
/// as it), or the server was working from another text than this one; and none
/// may overlap.
fn resolve(
    text: &Rope,
    edits: &[RenameEdit],
    check: Check,
    name: &str,
) -> Result<Vec<(Range<usize>, String)>, EditError> {
    let mut found = Vec::with_capacity(edits.len());
    for edit in edits {
        let start = from_lsp(text, edit.start);
        let end = from_lsp(text, edit.end);
        let stale = || EditError::Stale(name.to_owned());
        if end < start {
            return Err(stale());
        }
        if let Check::Rename(old) = check {
            let there = text.byte_slice(start..end).to_string();
            if there != old && there.strip_prefix("r#") != Some(old) {
                return Err(stale());
            }
        }
        found.push((start..end, edit.new_text.clone()));
    }
    found.sort_by_key(|(range, _)| (range.start, range.end));
    if found.windows(2).any(|pair| pair[1].0.start < pair[0].0.end) {
        return Err(EditError::Overlap(name.to_owned()));
    }
    Ok(found)
}

/// `text` with `replacements` (in text order, not overlapping) made.
fn edited(mut text: Rope, replacements: &[(Range<usize>, String)]) -> String {
    for (range, new) in replacements.iter().rev() {
        let start = text.byte_to_char(range.start);
        text.remove(start..text.byte_to_char(range.end));
        text.insert(start, new);
    }
    text.to_string()
}

/// Where a place in the text goes once `replacements` (in text order) are made;
/// a place inside a replaced stretch goes to the end of its new text.
pub(in crate::app) fn remap(byte: usize, replacements: &[(Range<usize>, String)]) -> usize {
    let mut shift = 0isize;
    for (range, new) in replacements {
        if range.end <= byte {
            shift += new.len() as isize - range.len() as isize;
        } else if range.start < byte {
            return range.start.saturating_add_signed(shift) + new.len();
        } else {
            break;
        }
    }
    byte.saturating_add_signed(shift)
}

impl Plan {
    /// Moves what the edit moves, in order, making the folders the new places are in. `step` is
    /// told what is about to move (the open tabs inside it are noted then) and what moved (the
    /// tabs follow it, as when a file is dragged in the explorer).
    pub(in crate::app) fn move_all(
        &self,
        mut step: impl FnMut(MoveStage, &Move),
    ) -> Result<(), EditError> {
        let mut done: Vec<&str> = Vec::new();
        for item in &self.moves {
            step(MoveStage::Before, item);
            let made = item
                .to
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| fs::rename(&item.from, &item.to));
            if let Err(source) = made {
                return Err(EditError::MoveFailed {
                    done: match done.is_empty() {
                        true => "nothing".to_owned(),
                        false => done.join(", "),
                    },
                    failed: item.from_name.clone(),
                    source,
                });
            }
            step(MoveStage::After, item);
            done.push(&item.to_name);
        }
        Ok(())
    }

    /// Writes the files that are not open: all of them to temporary files first,
    /// then each into its place.
    pub(in crate::app) fn write_closed(&mut self) -> Result<(), EditError> {
        let mut staged = Vec::with_capacity(self.closed.len());
        for file in &self.closed {
            let write =
                StagedWrite::stage(&file.path, &file.text).map_err(|source| EditError::Write {
                    name: file.name.clone(),
                    source,
                })?;
            staged.push(write);
        }
        let mut done: Vec<&str> = Vec::new();
        for (file, write) in self.closed.iter().zip(staged) {
            if let Err(source) = write.commit() {
                return Err(EditError::Torn {
                    done: match done.is_empty() {
                        true => "nothing".to_owned(),
                        false => done.join(", "),
                    },
                    failed: file.name.clone(),
                    source,
                });
            }
            done.push(&file.name);
        }
        Ok(())
    }
}

/// How a file is named in a message: from the project root when it is in it,
/// with `/` between the parts on every system.
pub(in crate::app) fn shown(root: &Path, path: &Path) -> String {
    let path = canonical(path);
    path.strip_prefix(root)
        .unwrap_or(&path)
        .display()
        .to_string()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests;
