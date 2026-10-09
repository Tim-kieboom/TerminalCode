//! Path helpers shared by the parts that compare paths from different places
//! (the explorer, open documents and the file watcher).

use std::path::{Path, PathBuf};

/// `path` with symlinks and `..` resolved, so two spellings of one file compare
/// equal. A path that does not exist (any more) has its directory resolved
/// instead, and stays as it is if even that fails.
pub(crate) fn canonical(path: &Path) -> PathBuf {
    if let Ok(resolved) = std::fs::canonicalize(path) {
        return resolved;
    }
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return path.to_path_buf();
    };
    let parent = match parent.as_os_str().is_empty() {
        true => Path::new("."),
        false => parent,
    };
    std::fs::canonicalize(parent).map_or_else(|_| path.to_path_buf(), |dir| dir.join(name))
}
