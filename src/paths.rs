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

/// `path` without the `\\?\` that Windows' `canonicalize` puts in front, which
/// many programs (`cmd.exe` among them) cannot use as a working directory. A
/// path with no such prefix, or one that cannot be written without it, comes
/// back as it is.
pub(crate) fn without_verbatim_prefix(path: &Path) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path.to_path_buf();
    };
    if let Some(share) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{share}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) if starts_with_drive(rest) => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

/// Whether `text` starts with a drive letter, `C:` or `C:\...`.
fn starts_with_drive(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes.get(2).is_none_or(|separator| *separator == b'\\')
}

#[cfg(test)]
mod tests;
