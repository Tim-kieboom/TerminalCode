//! Copying a file or a folder into a folder, for the explorer's paste.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::entries::EntryError;

/// How many numbered names are tried before it is given up.
const MOST_COPIES: usize = 10_000;

/// Copies `from` into the existing folder `folder`, under its own name, or, when that name is
/// taken, `name copy`, `name copy 2` and so on (`a copy.txt` for a file with an extension):
/// nothing is ever replaced. A folder is copied with everything in it; a symbolic link is copied
/// as a link and never followed, so a copy cannot leave the tree it is made from. A folder
/// cannot be copied into itself or into a folder inside it.
///
/// Returns the path of the copy. If it fails half way, what was copied until then is left as it
/// is, and the error names what could not be copied.
pub fn copy_into(from: &Path, folder: &Path) -> Result<PathBuf, EntryError> {
    let name = from
        .file_name()
        .ok_or_else(|| EntryError::Outside(from.display().to_string()))?;
    let shown = name.to_string_lossy().into_owned();
    let meta = fs::symlink_metadata(from).map_err(|source| EntryError::Io {
        name: shown.clone(),
        source,
    })?;
    if meta.is_dir() && is_inside(folder, from) {
        return Err(EntryError::IntoItself(shown));
    }
    let target = free_name(folder, name, meta.is_file()).ok_or_else(|| EntryError::Io {
        name: shown,
        source: io::Error::other("there are too many copies of it already"),
    })?;
    copy_entry(from, &target)?;
    Ok(target)
}

/// Whether `path` is `folder` or in it, the way the filesystem sees them.
fn is_inside(path: &Path, folder: &Path) -> bool {
    let whole = |path: &Path| fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    whole(path).starts_with(whole(folder))
}

/// A path in `folder` for a copy of an entry called `name` that nothing is at: `name` itself, or
/// with ` copy` and then ` copy 2`... put before the extension when `split_extension`.
pub fn free_name(folder: &Path, name: &OsStr, split_extension: bool) -> Option<PathBuf> {
    let taken = |path: &Path| fs::symlink_metadata(path).is_ok();
    let plain = folder.join(name);
    if !taken(&plain) {
        return Some(plain);
    }
    let path = Path::new(name);
    let (stem, extension) = match split_extension {
        true => (
            path.file_stem().unwrap_or(name),
            path.extension().map(OsStr::to_os_string),
        ),
        false => (name, None),
    };
    (1..=MOST_COPIES)
        .map(|number| {
            let mut copy = stem.to_os_string();
            copy.push(match number {
                1 => " copy".to_owned(),
                n => format!(" copy {n}"),
            });
            if let Some(extension) = &extension {
                copy.push(".");
                copy.push(extension);
            }
            folder.join(copy)
        })
        .find(|candidate| !taken(candidate))
}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> EntryError + '_ {
    move |source| EntryError::Io {
        name: path.display().to_string(),
        source,
    }
}

/// Copies one entry to `to`, which is not there: a link as a link, a folder and what is in it, a
/// file with its permissions. Anything else (a pipe, a socket) is refused.
fn copy_entry(from: &Path, to: &Path) -> Result<(), EntryError> {
    let meta = fs::symlink_metadata(from).map_err(io_error(from))?;
    let kind = meta.file_type();
    if kind.is_symlink() {
        let target = fs::read_link(from).map_err(io_error(from))?;
        return link(&target, from, to).map_err(io_error(to));
    }
    if kind.is_dir() {
        fs::create_dir(to).map_err(io_error(to))?;
        for entry in fs::read_dir(from).map_err(io_error(from))? {
            let entry = entry.map_err(io_error(from))?;
            copy_entry(&entry.path(), &to.join(entry.file_name()))?;
        }
        // After the files, so a folder that is read only can still be filled.
        return fs::set_permissions(to, meta.permissions()).map_err(io_error(to));
    }
    if kind.is_file() {
        return fs::copy(from, to).map(drop).map_err(io_error(to));
    }
    Err(io_error(from)(io::Error::other(
        "it is not a file, a folder or a link",
    )))
}

#[cfg(unix)]
fn link(target: &Path, _from: &Path, to: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, to)
}

#[cfg(windows)]
fn link(target: &Path, from: &Path, to: &Path) -> io::Result<()> {
    // The kind of link on Windows depends on what it points at, which is looked for from where
    // the link is.
    let points_at_a_folder = from
        .parent()
        .map(|parent| parent.join(target))
        .is_some_and(|resolved| resolved.is_dir());
    match points_at_a_folder {
        true => std::os::windows::fs::symlink_dir(target, to),
        false => std::os::windows::fs::symlink_file(target, to),
    }
}

#[cfg(test)]
mod tests;
