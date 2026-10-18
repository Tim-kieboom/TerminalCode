//! What can be done to the repository, as actions with arguments that are
//! checked here, not by whoever asks (a plugin, a key, the palette).

use std::path::{Component, Path};

use thiserror::Error;

use super::GitError;
use super::run::{Git, Outcome, failed, first_line};

/// Why an action was not done.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum ActionError {
    /// The argument cannot be right.
    #[error("{0}")]
    BadArgument(String),
    /// The remote wants a password, a passphrase or a key the editor cannot give it:
    /// what git said. Done once in the terminal pane (or with a credential helper
    /// or an ssh agent), it works from then on.
    #[error("{0}")]
    NeedsAuthentication(String),
    /// Another fetch, pull or push is running.
    #[error("another remote command is running")]
    Busy,
    /// A commit was asked for and nothing is staged.
    #[error("nothing is staged: stage a file first")]
    NothingStaged,
    /// `git` could not do it.
    #[error(transparent)]
    Git(#[from] GitError),
}

/// `path` as git writes it: from the root of the repository, with `/` between
/// the parts. Refused: nothing, a path from the root of the disk, one that goes
/// up (`..`), one in git's own folder.
pub(crate) fn repository_path(path: &str) -> Result<String, ActionError> {
    let bad = |why: &str| ActionError::BadArgument(format!("`{path}` {why}"));
    if path.is_empty() || path.contains('\0') {
        return Err(bad("is not a file name"));
    }
    let mut parts = Vec::new();
    for part in Path::new(path).components() {
        match part {
            Component::Normal(name) => parts.push(name.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => return Err(bad("goes out of the repository")),
            Component::RootDir | Component::Prefix(_) => {
                return Err(bad("is not from the root of the repository"));
            }
        }
    }
    if parts.is_empty() {
        return Err(bad("is not a file name"));
    }
    if parts.iter().any(|part| part == ".git") {
        return Err(bad("is in git's own folder"));
    }
    Ok(parts.join("/"))
}

impl Git {
    /// Stages the changes to the file at `path` (a deleted file stages its
    /// deletion). `root` is the top folder of the repository.
    pub(crate) fn stage(&self, root: &Path, path: &str) -> Result<(), ActionError> {
        self.change_index(root, "add", &["add", "--"], path)
    }

    /// Takes the file at `path` out of the staged changes, keeping what is in the
    /// working folder. (`reset` also works before the first commit.)
    pub(crate) fn unstage(&self, root: &Path, path: &str) -> Result<(), ActionError> {
        self.change_index(root, "reset", &["reset", "-q", "--"], path)
    }

    /// Stages every change in the working folder: new files, changes and deletions.
    pub(crate) fn stage_all(&self, root: &Path) -> Result<(), ActionError> {
        self.whole_index(root, "add", &["add", "-A"])
    }

    /// Takes everything out of the staged changes, keeping the working folder.
    pub(crate) fn unstage_all(&self, root: &Path) -> Result<(), ActionError> {
        self.whole_index(root, "reset", &["reset", "-q"])
    }

    /// Commits what is staged, with `message`. Refused: a message with nothing in
    /// it, and a commit with nothing staged. A hook that says no, or a missing name
    /// or email, fails with what git and the hook printed.
    pub(crate) fn commit(&self, root: &Path, message: &str) -> Result<(), ActionError> {
        let message = message.trim();
        if message.is_empty() {
            return Err(ActionError::BadArgument(
                "the commit message is empty".to_owned(),
            ));
        }
        if message.contains('\0') {
            return Err(ActionError::BadArgument(
                "the commit message has a character that cannot be in one".to_owned(),
            ));
        }
        // `--quiet` makes the exit code the answer: 1 is "there are changes".
        let staged = self.run(root, &["diff", "--cached", "--quiet"])?;
        match staged.code {
            Some(0) => return Err(ActionError::NothingStaged),
            Some(1) => {}
            _ => return Err(failed("diff", first_line(&staged.stderr)).into()),
        }
        let outcome = self.run_write(root, &["commit", "-m", message])?;
        match outcome.success {
            true => Ok(()),
            false => Err(failed("commit", output_of(&outcome)).into()),
        }
    }

    fn whole_index(&self, root: &Path, name: &str, args: &[&str]) -> Result<(), ActionError> {
        let outcome = self.run_write(root, args)?;
        match outcome.success {
            true => Ok(()),
            false => Err(failed(name, first_line(&outcome.stderr)).into()),
        }
    }

    fn change_index(
        &self,
        root: &Path,
        name: &str,
        args: &[&str],
        path: &str,
    ) -> Result<(), ActionError> {
        let path = repository_path(path)?;
        let mut call = args.to_vec();
        call.push(&path);
        let outcome = self.run_write(root, &call)?;
        match outcome.success {
            true => Ok(()),
            false => Err(failed(name, first_line(&outcome.stderr)).into()),
        }
    }
}

/// What a command printed, for showing why it failed: the lines of what it wrote
/// to stderr and then stdout (a hook may use either), joined, and cut short.
pub(super) fn output_of(outcome: &Outcome) -> String {
    const MOST_LINES: usize = 12;
    const MOST_CHARS: usize = 600;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&outcome.stderr),
        String::from_utf8_lossy(&outcome.stdout)
    );
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let mut joined = lines
        .iter()
        .take(MOST_LINES)
        .copied()
        .collect::<Vec<_>>()
        .join(" / ");
    if lines.len() > MOST_LINES || joined.chars().count() > MOST_CHARS {
        joined = joined.chars().take(MOST_CHARS).collect::<String>() + "...";
    }
    if joined.is_empty() {
        joined = "it said nothing".to_owned();
    }
    joined
}
