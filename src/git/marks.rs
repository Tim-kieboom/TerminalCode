//! What to show beside a file: a color and a letter for the files git lists,
//! and for the folders that have such files inside. Built once for each
//! [`Status`], read for every row and tab that is drawn.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{Change, Kind, Status};
use crate::files::paths::{canonical, without_verbatim_prefix};

/// The color a mark has. The order is how strongly it shows on a folder that
/// has files of several kinds inside: the strongest wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum MarkKind {
    Untracked,
    Added,
    Modified,
    Deleted,
    Conflict,
}

/// What shows beside one file or folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Mark {
    pub kind: MarkKind,
    /// `M`, `A`, `D`, `R`, `U` or `!`; the letter of a folder is not shown.
    pub letter: char,
}

impl Mark {
    /// The theme slot that colors it.
    pub(crate) fn slot(self) -> &'static str {
        match self.kind {
            MarkKind::Untracked => "git.untracked",
            MarkKind::Added => "git.added",
            MarkKind::Modified => "git.modified",
            MarkKind::Deleted => "git.deleted",
            MarkKind::Conflict => "git.conflict",
        }
    }

    fn of(change: Change) -> Self {
        match change {
            Change::Untracked => Self::new(MarkKind::Untracked, 'U'),
            Change::Conflicted => Self::new(MarkKind::Conflict, '!'),
            Change::Tracked { staged, unstaged } => [staged, unstaged]
                .into_iter()
                .flatten()
                .map(Self::of_kind)
                .max_by_key(|mark| mark.kind)
                // `git` lists a tracked file only when one side changed.
                .unwrap_or(Self::new(MarkKind::Modified, 'M')),
        }
    }

    fn of_kind(kind: Kind) -> Self {
        match kind {
            Kind::Modified | Kind::TypeChanged => Self::new(MarkKind::Modified, 'M'),
            Kind::Deleted => Self::new(MarkKind::Deleted, 'D'),
            Kind::Added | Kind::Copied => Self::new(MarkKind::Added, 'A'),
            Kind::Renamed => Self::new(MarkKind::Added, 'R'),
        }
    }

    fn new(kind: MarkKind, letter: char) -> Self {
        Self { kind, letter }
    }
}

/// The marks of every changed file and of the folders above them.
#[derive(Debug, Default)]
pub(crate) struct Marks {
    /// The top folder of the repository; empty before the first status.
    root: PathBuf,
    /// The files git lists, from the root of the repository.
    files: HashMap<PathBuf, Mark>,
    /// The folders that have changed files inside, with the strongest mark of those.
    folders: HashMap<PathBuf, Mark>,
}

impl Marks {
    pub(crate) fn from_status(status: &Status) -> Self {
        let mut marks = Self {
            root: without_verbatim_prefix(&status.root),
            ..Self::default()
        };
        for entry in &status.entries {
            let mark = Mark::of(entry.change);
            marks.files.insert(entry.path.clone(), mark);
            // A folder listed as one entry (all its files are untracked) ends
            // with a separator; its own path is the one without it.
            for folder in entry.path.ancestors().skip(1) {
                if folder.as_os_str().is_empty() {
                    break;
                }
                let strongest = marks.folders.entry(folder.to_path_buf()).or_insert(mark);
                if mark.kind > strongest.kind {
                    *strongest = mark;
                }
            }
        }
        marks
    }

    /// The mark of the file or folder at `path`, which is a path in the
    /// repository however it is spelled (a link, a relative path: such a path
    /// is looked up by where it really is, which asks the file system, so
    /// callers with many paths give them as they are under the root).
    pub(crate) fn of(&self, path: &Path) -> Option<Mark> {
        if self.files.is_empty() {
            return None;
        }
        match path.strip_prefix(&self.root) {
            Ok(inside) => self.inside(inside),
            Err(_) => {
                let real = without_verbatim_prefix(&canonical(path));
                self.inside(real.strip_prefix(&self.root).ok()?)
            }
        }
    }

    /// The mark of a path from the root of the repository: its own, or the one
    /// of the folder it is in when git lists that folder as one entry.
    fn inside(&self, relative: &Path) -> Option<Mark> {
        if let Some(mark) = self.files.get(relative) {
            return Some(*mark);
        }
        if let Some(mark) = self.folders.get(relative) {
            return Some(*mark);
        }
        relative
            .ancestors()
            .skip(1)
            .find_map(|folder| self.files.get(folder).copied())
            .filter(|mark| mark.kind == MarkKind::Untracked)
    }
}
