//! Open File and Open Folder from the menu or the palette: a prompt for a path.
//! A file opens as a tab; a folder cannot be swapped in under a running editor
//! (the explorer, the watcher, the language servers and the shells all hold the
//! project root), so the editor ends and the program starts again in it.

use std::fs;
use std::path::{Path, PathBuf};

use super::App;
use crate::components::popups::name_prompt::NamePrompt;
use crate::entries::EntryError;

/// `text` as a path: `~` and `~/` stand for the home folder and a relative path
/// is from `base`.
pub(crate) fn expand_path(text: &str, base: &Path) -> PathBuf {
    let text = text.trim();
    let home = || dirs::home_dir().unwrap_or_default();
    if text == "~" {
        return home();
    }
    if let Some(rest) = text.strip_prefix("~/").or_else(|| text.strip_prefix("~\\")) {
        return home().join(rest);
    }
    let path = PathBuf::from(text);
    match path.is_absolute() {
        true => path,
        false => base.join(path),
    }
}

impl App {
    /// Asks for the path of a file to open.
    pub(super) fn ask_for_file(&mut self) {
        let prompt = NamePrompt::open_file(self.project_root());
        self.state.open_name_prompt(prompt);
    }

    /// Asks for the path of a folder to open.
    pub(super) fn ask_for_folder(&mut self) {
        let prompt = NamePrompt::open_folder(self.project_root());
        self.state.open_name_prompt(prompt);
    }

    /// Opens the file at the typed path as a tab.
    pub(super) fn open_typed_file(&mut self, base: &Path, typed: &str) -> Result<(), EntryError> {
        if typed.trim().is_empty() {
            return Err(EntryError::Empty);
        }
        let path = expand_path(typed, base);
        self.open_file_at(&path)
    }

    /// Opens `path` as a tab, which has to be a file that exists.
    pub(super) fn open_file_at(&mut self, path: &Path) -> Result<(), EntryError> {
        let name = path.display().to_string();
        let meta = fs::metadata(path).map_err(|source| EntryError::Io {
            name: name.clone(),
            source,
        })?;
        if !meta.is_file() {
            return Err(EntryError::NotAFile(name));
        }
        match self.state.workspace_mut().open_path(path) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.state.notify_error(error.to_string());
                Ok(())
            }
        }
    }

    /// Ends the editor so that the program starts again in the typed folder.
    /// Unsaved files are asked about first, as for quit.
    pub(super) fn restart_in_typed_folder(
        &mut self,
        base: &Path,
        typed: &str,
    ) -> Result<(), EntryError> {
        if typed.trim().is_empty() {
            return Err(EntryError::Empty);
        }
        let path = expand_path(typed, base);
        self.restart_in_folder(&path)
    }

    /// Starts again in `path`, which has to be a folder that exists.
    pub(super) fn restart_in_folder(&mut self, path: &Path) -> Result<(), EntryError> {
        let name = path.display().to_string();
        let meta = fs::metadata(path).map_err(|source| EntryError::Io {
            name: name.clone(),
            source,
        })?;
        if !meta.is_dir() {
            return Err(EntryError::NotAFolder(name));
        }
        self.restart_in = Some(crate::paths::canonical(path));
        self.quit();
        Ok(())
    }

    /// The folder the program starts again in after this run, if it was asked to.
    #[cfg(test)]
    pub(crate) fn restart_target(&self) -> Option<&Path> {
        self.restart_in.as_deref()
    }
}
