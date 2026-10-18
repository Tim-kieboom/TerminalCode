//! What can be done to the repository, as actions with arguments that are
//! checked here, not by whoever asks (a plugin, a key, the palette).

use std::path::{Component, Path};

use thiserror::Error;

use super::GitError;
use super::run::{Git, failed, first_line};

/// Why an action was not done.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum ActionError {
    /// The argument cannot be right.
    #[error("{0}")]
    BadArgument(String),
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
