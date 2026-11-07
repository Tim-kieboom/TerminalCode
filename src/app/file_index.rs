//! The project's files by name, for a path in the terminal's output that is not where it says it
//! is (`mod.rs:3` from a build run in another folder): when exactly one file of the project ends
//! with the text, that is the file. The list is read on a thread of its own the first time a
//! path needs it, and again when it is old.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::App;
use crate::components::popups::finder::list_files;
use crate::event::Event;

/// Files kept at most; a project with more has none found this way past them.
const MOST_FILES: usize = 300_000;
/// How long a list is used before it is read again (the next time a path needs it).
const OLD_AFTER: Duration = Duration::from_secs(30);

/// What the app knows of the project's files.
#[derive(Debug, Default)]
pub(crate) struct FileIndex {
    /// The folder the files are of, and them with `/` between the parts.
    root: Option<PathBuf>,
    files: Vec<String>,
    read_at: Option<Instant>,
    /// A read is going on.
    reading: bool,
}

impl FileIndex {
    /// The only file of the project that is `text` or ends with `/text`, from the project folder.
    pub(crate) fn unique_ending(&self, text: &str) -> Option<PathBuf> {
        let root = self.root.as_ref()?;
        let wanted = normalized(text)?;
        let found = unique_with_ending(&self.files, &wanted)?;
        Some(root.join(found))
    }

    /// Whether a read should start for `root`: none is going on, and the list is of another
    /// folder, not read yet or old.
    fn wants_read(&self, root: &Path) -> bool {
        !self.reading
            && (self.root.as_deref() != Some(root)
                || self.read_at.is_none_or(|at| at.elapsed() > OLD_AFTER))
    }
}

/// `text` as the project lists its files: `/` between the parts, no `./` in front. `None` for a
/// path that is whole or leaves its folder, which no ending can name.
pub(crate) fn normalized(text: &str) -> Option<String> {
    let text = text.replace('\\', "/");
    let text = text.trim_start_matches("./");
    let has_parent = text.split('/').any(|part| part == "..");
    (!text.is_empty() && !text.starts_with('/') && !has_parent && !text.contains(':'))
        .then(|| text.to_owned())
}

/// The file that is `wanted` itself, or else the only one that ends with `/wanted`; `None` for
/// none or several.
pub(crate) fn unique_with_ending(files: &[String], wanted: &str) -> Option<String> {
    if let Some(exact) = files.iter().find(|file| *file == wanted) {
        return Some(exact.clone());
    }
    let mut found = None;
    for file in files {
        let matches = file
            .strip_suffix(wanted)
            .is_some_and(|before| before.ends_with('/'));
        if !matches {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some(file.clone());
    }
    found
}

impl App {
    /// Reads the project's files on a thread when a path asked for them and the list is not
    /// there (or is old). The answer comes as [`Event::FileIndex`].
    pub(super) fn want_file_index(&mut self) {
        let root = self.project_root();
        if !self.file_index.wants_read(&root) {
            return;
        }
        let Some(events) = self.background.sender() else {
            return;
        };
        self.file_index.reading = true;
        std::thread::spawn(move || {
            let files = list_files(&root, MOST_FILES)
                .into_iter()
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .collect();
            let _ = events.blocking_send(Event::FileIndex { root, files });
        });
    }

    /// The files were read.
    pub(super) fn file_index_ready(&mut self, root: PathBuf, files: Vec<String>) {
        self.file_index = FileIndex {
            root: Some(root),
            files,
            read_at: Some(Instant::now()),
            reading: false,
        };
        self.needs_redraw = true;
    }

    /// The file of the project that a path ends like, when it is the only one.
    pub(super) fn unique_project_file(&self, text: &str) -> Option<PathBuf> {
        self.file_index.unique_ending(text)
    }
}

#[cfg(test)]
mod tests;
