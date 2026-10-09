//! Making new files and folders inside the project.

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum EntryError {
    #[error("type a name first")]
    Empty,
    #[error("{0} is not inside the folder")]
    Outside(String),
    #[error("{0} already exists")]
    Exists(String),
    #[error("a folder cannot be moved into itself ({0})")]
    IntoItself(String),
    #[error("{name}: {source}")]
    Io { name: String, source: io::Error },
}

/// What to make.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryKind {
    File,
    Folder,
}

/// Makes the file or folder `name` in `dir` and returns its path. `name` may
/// have several parts (`a/b/c.rs`): the folders before the last are made as
/// needed. An entry that already exists is never touched.
pub(crate) fn create(dir: &Path, name: &str, kind: EntryKind) -> Result<PathBuf, EntryError> {
    let name = name.trim();
    let relative = inside(name)?;
    let path = dir.join(&relative);
    let io_error = |source| EntryError::Io {
        name: name.to_owned(),
        source,
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let made = match kind {
        EntryKind::File => OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map(drop),
        EntryKind::Folder => fs::create_dir(&path),
    };
    match made {
        Ok(()) => Ok(path),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            Err(EntryError::Exists(name.to_owned()))
        }
        Err(error) => Err(io_error(error)),
    }
}

/// `name` as a path that stays inside the folder it is joined to: no `..`, no
/// root, nothing empty.
fn inside(name: &str) -> Result<PathBuf, EntryError> {
    if name.is_empty() {
        return Err(EntryError::Empty);
    }
    let mut relative = PathBuf::new();
    for component in Path::new(name).components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            _ => return Err(EntryError::Outside(name.to_owned())),
        }
    }
    match relative.as_os_str().is_empty() {
        true => Err(EntryError::Empty),
        false => Ok(relative),
    }
}

/// Renames `from` to `name`, which is relative to the folder `from` is in and
/// may have several parts (the folders are made as needed). An entry that is
/// already there is never replaced.
pub(crate) fn rename(from: &Path, name: &str) -> Result<PathBuf, EntryError> {
    let dir = from
        .parent()
        .ok_or_else(|| EntryError::Outside(name.trim().to_owned()))?;
    relocate(from, dir, name, false)
}

/// Moves `from` to the path `name`, which is relative to `root`. Like `mv`, a
/// `name` that is an existing folder means "into it". Missing folders are made
/// and an entry that is already there is never replaced.
pub(crate) fn move_to(from: &Path, root: &Path, name: &str) -> Result<PathBuf, EntryError> {
    relocate(from, root, name, true)
}

fn relocate(
    from: &Path,
    dir: &Path,
    name: &str,
    into_folders: bool,
) -> Result<PathBuf, EntryError> {
    let name = name.trim();
    let mut to = dir.join(inside(name)?);
    if into_folders
        && to != from
        && to.is_dir()
        && let Some(own_name) = from.file_name()
    {
        to.push(own_name);
    }
    if to == from {
        return Ok(to);
    }
    if to.starts_with(from) {
        return Err(EntryError::IntoItself(name.to_owned()));
    }
    let io_error = |source| EntryError::Io {
        name: name.to_owned(),
        source,
    };
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    // Where the filesystem cannot refuse atomically this checks first and
    // renames after, which a file made in between can slip through.
    match renamore::rename_exclusive_fallback(from, &to) {
        Ok(_) => Ok(to),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            Err(EntryError::Exists(name.to_owned()))
        }
        Err(error) => Err(io_error(error)),
    }
}
