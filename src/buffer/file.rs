use std::fs;
use std::hash::Hasher;
use std::io;
use std::path::{Path, PathBuf};

use ropey::Rope;
use thiserror::Error;

use super::{Buffer, History, LineEnding};

#[derive(Debug, Error)]
pub enum FileError {
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error("{}: file is not valid UTF-8", path.display())]
    NotUtf8 { path: PathBuf },
    #[error("buffer has no file path; use save as")]
    NoPath,
}

/// What the file on disk held when the buffer last read or wrote it, so a later
/// look can tell the buffer's own saves from changes made by someone else.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Disk {
    /// The buffer has not been tied to a file.
    #[default]
    Unknown,
    /// The file did not exist.
    Missing,
    /// The file existed, with contents that hash to this.
    Content(u64),
}

/// How the file on disk now differs from what the buffer last read or wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiskChange {
    /// Someone else changed the contents (the hash identifies this state, so
    /// the same change is recognized again).
    Modified(u64),
    Deleted,
}

fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    hasher.write(bytes);
    hasher.finish()
}

impl FileError {
    fn io(path: &Path, source: io::Error) -> Self {
        Self::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}

impl Buffer {
    /// Reads a UTF-8 file into a clean buffer.
    pub(crate) fn open(path: impl Into<PathBuf>) -> Result<Self, FileError> {
        let path = path.into();
        let bytes = fs::read(&path).map_err(|source| FileError::io(&path, source))?;
        let disk = Disk::Content(fingerprint(&bytes));
        let Ok(text) = String::from_utf8(bytes) else {
            return Err(FileError::NotUtf8 { path });
        };

        let mut buffer = Self::from_text(&text);
        buffer.path = Some(path);
        buffer.disk = disk;
        Ok(buffer)
    }

    /// Like [`Buffer::open`], but a missing file gives an empty buffer that
    /// will create the file when saved.
    pub(crate) fn open_or_new(path: impl Into<PathBuf>) -> Result<Self, FileError> {
        let path = path.into();
        match Self::open(&path) {
            Err(FileError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => {
                Ok(Self {
                    path: Some(path),
                    disk: Disk::Missing,
                    ..Self::default()
                })
            }
            other => other,
        }
    }

    /// Whether someone else changed the file since the buffer last read or
    /// wrote it. A buffer with no file, or one never tied to a file, has
    /// nothing to compare with.
    pub(crate) fn disk_change(&self) -> Result<Option<DiskChange>, FileError> {
        let Some(path) = &self.path else {
            return Ok(None);
        };
        if self.disk == Disk::Unknown {
            return Ok(None);
        }
        match fs::read(path) {
            Ok(bytes) => {
                let now = fingerprint(&bytes);
                match self.disk == Disk::Content(now) {
                    true => Ok(None),
                    false => Ok(Some(DiskChange::Modified(now))),
                }
            }
            Err(source) if source.kind() == io::ErrorKind::NotFound => match self.disk {
                Disk::Content(_) => Ok(Some(DiskChange::Deleted)),
                Disk::Missing | Disk::Unknown => Ok(None),
            },
            Err(source) => Err(FileError::io(path, source)),
        }
    }

    /// Replaces the text with what is on disk and forgets the undo history,
    /// which no longer describes the text. The buffer is clean afterwards.
    /// Positions into the old text are the caller's to fix up.
    pub(crate) fn reload(&mut self) -> Result<(), FileError> {
        let Some(path) = self.path.clone() else {
            return Err(FileError::NoPath);
        };
        let bytes = fs::read(&path).map_err(|source| FileError::io(&path, source))?;
        let disk = Disk::Content(fingerprint(&bytes));
        let Ok(text) = String::from_utf8(bytes) else {
            return Err(FileError::NotUtf8 { path });
        };

        self.rope = Rope::from_str(&text);
        self.line_ending = LineEnding::detect(&text);
        self.version += 1;
        self.saved_version = self.version;
        self.history = History::default();
        self.edit_log.clear();
        self.disk = disk;
        Ok(())
    }

    /// Makes `path` the file this buffer belongs to, because the file was moved
    /// there. The text and what the buffer knows of the disk stay as they are.
    pub(crate) fn set_path(&mut self, path: PathBuf) {
        self.path = Some(path);
    }

    /// Writes the buffer to its own path.
    pub(crate) fn save(&mut self) -> Result<(), FileError> {
        let Some(path) = self.path.clone() else {
            return Err(FileError::NoPath);
        };
        self.save_as(path)
    }

    /// Writes the buffer to `path` and makes that its path. The write goes to
    /// a temporary file that replaces the target, so a failure never leaves a
    /// half-written file.
    pub(crate) fn save_as(&mut self, path: impl Into<PathBuf>) -> Result<(), FileError> {
        let path = path.into();
        let text = self.text();
        write_atomic(&path, &text)?;
        self.disk = Disk::Content(fingerprint(text.as_bytes()));
        self.path = Some(path);
        self.saved_version = self.version;
        Ok(())
    }
}

fn write_atomic(path: &Path, text: &str) -> Result<(), FileError> {
    // Write through symlinks instead of replacing them.
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let temp = temp_path_for(&target).ok_or_else(|| {
        FileError::io(
            path,
            io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"),
        )
    })?;

    let result = write_then_replace(&temp, &target, text);
    if result.is_err() {
        // Best effort: the original error matters more than a leftover file.
        let _ = fs::remove_file(&temp);
    }
    result.map_err(|source| FileError::io(path, source))
}

fn write_then_replace(temp: &Path, target: &Path, text: &str) -> io::Result<()> {
    fs::write(temp, text)?;
    if let Ok(metadata) = fs::metadata(target) {
        fs::set_permissions(temp, metadata.permissions())?;
    }
    fs::rename(temp, target)
}

fn temp_path_for(target: &Path) -> Option<PathBuf> {
    let name = target.file_name()?.to_string_lossy();
    let temp_name = format!(".{name}.{}.tmp", std::process::id());
    Some(target.with_file_name(temp_name))
}
