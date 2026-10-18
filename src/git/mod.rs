//! Git, read through the user's own `git` command line: where the repository
//! is, which branch is checked out and which files changed. Nothing here draws
//! anything; the explorer, the tabs and the status bar read a [`Status`]. See
//! "Git" in `docs/design.md`.
//!
//! Every read-only call takes no locks (see [`Git`]), so a refresh in the
//! background never collides with a `git` the user runs in the terminal pane.

use std::path::PathBuf;
use std::time::Duration;

use thiserror::Error;

pub(crate) use actions::{ActionError, repository_path};
pub(crate) use diff::{HunkKind, hash_lines};
pub(crate) use lines::Lines;
pub(crate) use marks::{Mark, Marks};
pub(crate) use parse::parse_status;
pub(crate) use run::Git;
pub(crate) use schedule::Schedule;

mod actions;
mod diff;
mod lines;
mod marks;
mod parse;
mod run;
mod schedule;
#[cfg(test)]
mod tests;

/// What happened to a file, as far as one side of the index is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Kind {
    Modified,
    TypeChanged,
    Added,
    Deleted,
    Renamed,
    Copied,
}

/// How a file differs from the last commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Change {
    /// A known file: what was staged, and what changed since it was staged.
    Tracked {
        staged: Option<Kind>,
        unstaged: Option<Kind>,
    },
    /// A file git does not know yet (a folder with only such files is one entry,
    /// its name ending in a separator).
    Untracked,
    /// A merge left the file with conflicts.
    Conflicted,
}

/// One changed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The path from the root of the repository.
    pub path: PathBuf,
    /// Where a renamed or copied file came from.
    pub original: Option<PathBuf>,
    pub change: Change,
}

/// The checked-out branch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Branch {
    /// The branch name; `None` when `HEAD` is detached.
    pub name: Option<String>,
    /// The commit `HEAD` is at; `None` before the first commit.
    pub oid: Option<String>,
    /// The branch it follows, such as `origin/main`.
    pub upstream: Option<String>,
    /// How many commits it has that the upstream has not, and the other way round.
    pub ahead: u32,
    pub behind: u32,
}

impl Branch {
    /// What to call it for a person: the name, or the start of the commit when
    /// `HEAD` is detached.
    pub(crate) fn label(&self) -> String {
        match (&self.name, &self.oid) {
            (Some(name), _) => name.clone(),
            (None, Some(oid)) => format!("({})", oid.get(..7).unwrap_or(oid)),
            (None, None) => "(no commit)".to_owned(),
        }
    }
}

/// The state of the repository at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Status {
    /// The top folder of the repository (the project may be inside it).
    pub root: PathBuf,
    pub branch: Branch,
    /// The changed files, in the order `git` lists them.
    pub entries: Vec<Entry>,
}

/// Why git could not be asked.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum GitError {
    #[error("`git` was not found on the PATH")]
    NotInstalled,
    /// The folder is not in a repository, or `git` refuses to look at it; the
    /// text is what `git` said.
    #[error("{0}")]
    NoRepository(String),
    #[error("`git` took longer than {} seconds", .0.as_secs())]
    TimedOut(Duration),
    #[error("`git {command}` failed: {detail}")]
    Failed { command: String, detail: String },
    #[error("`git` said something that was not understood: {0}")]
    Unreadable(String),
}

impl GitError {
    /// Whether asking again later cannot help: the feature stays off.
    pub(crate) fn is_final(&self) -> bool {
        matches!(self, Self::NotInstalled | Self::NoRepository(_))
    }
}
