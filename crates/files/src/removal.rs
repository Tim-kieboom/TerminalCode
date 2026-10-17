//! Taking files and folders away: to the OS trash, or for good.

use std::io;
use std::path::Path;

/// Moves `path` to the OS trash. The error is the reason, ready to show.
pub type TrashFn = fn(&Path) -> Result<(), String>;

pub fn move_to_trash(path: &Path) -> Result<(), String> {
    trash::delete(path).map_err(|error| error.to_string())
}

/// Deletes `path` for good: a file, a link, or a folder with everything in it.
/// A link to a folder is removed, not what it points to.
pub fn remove_permanently(path: &Path) -> io::Result<()> {
    let kind = std::fs::symlink_metadata(path)?.file_type();
    match kind.is_dir() {
        true => std::fs::remove_dir_all(path),
        false => std::fs::remove_file(path),
    }
}
