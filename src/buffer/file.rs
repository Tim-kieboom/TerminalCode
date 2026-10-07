use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::Buffer;

#[derive(Debug, Error)]
pub enum FileError {
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error("{}: file is not valid UTF-8", path.display())]
    NotUtf8 { path: PathBuf },
    #[error("buffer has no file path; use save as")]
    NoPath,
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
        let Ok(text) = String::from_utf8(bytes) else {
            return Err(FileError::NotUtf8 { path });
        };

        let mut buffer = Self::from_text(&text);
        buffer.path = Some(path);
        Ok(buffer)
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
        write_atomic(&path, &self.text())?;
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
