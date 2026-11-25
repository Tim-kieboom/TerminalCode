//! The text of documents with unsaved edits, written when the terminal goes away under
//! the editor (a hangup: the window was closed without quitting) and offered back at the
//! next start in the same project folder.
//!
//! One file for each document in the `recovery` folder of the config folder, named by a
//! hash of the document's path, so a second hangup replaces the first one's file. The
//! text may hold secrets, so on Unix the files and the folder are private to the user.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The folder of the recovery files, inside the config folder.
pub(crate) const FOLDER: &str = "recovery";

/// The most text of one document that is written. A larger one is skipped: a hangup
/// leaves little time, and such a file is better found on disk than rebuilt from here.
pub(crate) const MAX_BYTES: usize = 10 * 1024 * 1024;

/// The version of the file's layout; a file of a newer version is left alone.
const LAYOUT: u32 = 1;

/// What is kept of one document with unsaved edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Recovery {
    #[serde(default = "layout")]
    version: u32,
    /// The project folder it was open in, a whole path.
    pub(crate) project: PathBuf,
    /// The file the document belongs to; `None` for text that was never saved.
    pub(crate) file: Option<PathBuf>,
    /// What an unsaved text was called, if it had a name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) title: Option<String>,
    pub(crate) text: String,
    /// The hash of the file as the document last read or wrote it (see
    /// `Buffer::disk_fingerprint`); `None` when the file did not exist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) disk: Option<u64>,
    /// Where the cursor was.
    pub(crate) line: usize,
    pub(crate) column: usize,
}

fn layout() -> u32 {
    LAYOUT
}

impl Recovery {
    pub(crate) fn new(project: &Path, file: Option<&Path>, text: String) -> Self {
        Self {
            version: LAYOUT,
            project: project.to_path_buf(),
            file: file.map(Path::to_path_buf),
            title: None,
            text,
            disk: None,
            line: 0,
            column: 0,
        }
    }

    /// Records the hash of the file as the document last saw it.
    pub(crate) fn with_disk(mut self, disk: Option<u64>) -> Self {
        self.disk = disk;
        self
    }

    /// Records the name of an unsaved text.
    pub(crate) fn with_title(mut self, title: Option<&str>) -> Self {
        self.title = title.map(str::to_owned);
        self
    }

    /// Records where the cursor was.
    pub(crate) fn with_cursor(mut self, line: usize, column: usize) -> Self {
        self.line = line;
        self.column = column;
        self
    }

    /// What the question calls the document.
    pub(crate) fn name(&self) -> String {
        let named = self
            .file
            .as_deref()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned());
        named
            .or_else(|| self.title.clone())
            .unwrap_or_else(|| "[no name]".to_owned())
    }
}

/// A recovery file that was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Found {
    /// Where the file is, to remove it once it is dealt with.
    pub(crate) path: PathBuf,
    pub(crate) recovery: Recovery,
}

/// What a look in the recovery folder gave.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Search {
    /// The files of this project, by name.
    pub(crate) found: Vec<Found>,
    /// Files that could not be used; they are left where they are.
    pub(crate) problems: Vec<String>,
}

/// Writes `items`, a file for each one with at most [`MAX_BYTES`] of text. Every one is
/// tried; the error is the first that came up. Returns how many were written.
pub(crate) fn write(config_dir: &Path, items: &[Recovery]) -> io::Result<usize> {
    let folder = config_dir.join(FOLDER);
    let mut written = 0;
    let mut first_error = None;
    let mut untitled = 0;
    for item in items {
        let key = match &item.file {
            Some(file) => file.to_string_lossy().into_owned(),
            None => {
                untitled += 1;
                format!("untitled-{untitled}")
            }
        };
        if item.text.len() > MAX_BYTES {
            continue;
        }
        match write_one(&folder, &key, item) {
            Ok(()) => written += 1,
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(written),
    }
}

/// The recovery files of the project folder `project` in `config_dir`.
pub(crate) fn find(config_dir: &Path, project: &Path) -> Search {
    let mut search = Search::default();
    let Ok(entries) = fs::read_dir(config_dir.join(FOLDER)) else {
        return search;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    paths.sort();
    for path in paths {
        match read(&path) {
            Ok(recovery) if recovery.project == project => {
                search.found.push(Found { path, recovery });
            }
            Ok(_) => {}
            Err(problem) => search
                .problems
                .push(format!("{}: {problem}", path.display())),
        }
    }
    search
}

/// Removes recovery files that were dealt with. A file that is already gone is fine.
pub(crate) fn remove(paths: &[PathBuf]) -> io::Result<()> {
    let mut first_error = None;
    for path in paths {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn read(path: &Path) -> Result<Recovery, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let recovery: Recovery = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    if recovery.version > LAYOUT {
        return Err(format!(
            "written by a newer version of the editor (layout {})",
            recovery.version
        ));
    }
    Ok(recovery)
}

fn write_one(folder: &Path, key: &str, item: &Recovery) -> io::Result<()> {
    let text = serde_json::to_string(item).map_err(io::Error::other)?;
    make_private_folder(folder)?;
    let name = file_name(&item.project, key);
    let path = folder.join(&name);
    let temporary = folder.join(format!("{name}.tmp"));
    write_private(&temporary, &text)?;
    fs::rename(&temporary, &path).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })
}

/// The name of the file of a document: a hash of its project and its key, so the same
/// document always gets the same file, and nothing in a path can make a name that
/// escapes the folder.
fn file_name(project: &Path, key: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(project.to_string_lossy().as_bytes());
    hash.update([0]);
    hash.update(key.as_bytes());
    let digest = hash.finalize();
    let mut name = String::with_capacity(21);
    for byte in digest.iter().take(8) {
        name.push_str(&format!("{byte:02x}"));
    }
    name.push_str(".json");
    name
}

#[cfg(unix)]
fn make_private_folder(folder: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(folder)
}

#[cfg(not(unix))]
fn make_private_folder(folder: &Path) -> io::Result<()> {
    fs::create_dir_all(folder)
}

#[cfg(unix)]
fn write_private(path: &Path, text: &str) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(text.as_bytes())
}

#[cfg(not(unix))]
fn write_private(path: &Path, text: &str) -> io::Result<()> {
    fs::write(path, text)
}
