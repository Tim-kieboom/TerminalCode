//! The text of the file in front, pushed to the plugins that have `files.text`: the
//! saved text on disk, only of the kinds the manifest names and the user left on,
//! at most 1 MB. A plugin cannot ask for a file, only be handed this one. See
//! "Markdown preview" in `docs/design.md`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::App;
use crate::files::paths::canonical;
use crate::plugin_api::{Capability, MAX_FILE_TEXT, MessageError, ToPlugin};

/// What a plugin was last told about the file in front, so that it is told again only
/// when something changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Pushed {
    /// Nothing: no file, or not one of its kinds.
    Cleared,
    File(FileKey),
}

/// What tells that the file in front, or its text, is not what the plugin was told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FileKey {
    path: PathBuf,
    modified: bool,
    saves: u64,
    reloads: u64,
    changed: Option<SystemTime>,
    len: u64,
}

/// The file of the tab in front.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActiveFile {
    pub(crate) path: PathBuf,
    /// The buffer has changes that are not saved.
    pub(crate) modified: bool,
    pub(crate) saves: u64,
    pub(crate) reloads: u64,
}

/// Whether `path` has one of the extensions `kinds` (lower case, no dot).
pub(super) fn has_kind(path: &Path, kinds: &[String]) -> bool {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|extension| kinds.contains(&extension))
}

/// What the manifest names, narrowed to what the user's setting (when it names the
/// plugin) leaves. The setting can only take kinds away.
pub(super) fn effective_kinds(manifest: &[String], narrowed: Option<&Vec<String>>) -> Vec<String> {
    manifest
        .iter()
        .map(|kind| kind.to_ascii_lowercase())
        .filter(|kind| narrowed.is_none_or(|allowed| allowed.contains(kind)))
        .collect()
}

impl App {
    /// The plugins that run with `files.text`, with the kinds each may get.
    fn file_text_receivers(&self) -> Vec<(Box<str>, Vec<String>)> {
        self.plugins
            .manager
            .running_ids()
            .filter_map(|id| {
                let installed = self.plugins.manager.installed(&id)?;
                if !installed.capabilities.contains(&Capability::FilesText) {
                    return None;
                }
                let kinds = effective_kinds(
                    &installed.manifest.file_types,
                    self.settings.plugin_file_types.get(&*id),
                );
                Some((id, kinds))
            })
            .collect()
    }

    /// Tells the plugins that get file text what is in front now, when that is not
    /// what they were told. Cheap when none runs: it is called after every input.
    pub(super) fn push_file_text(&mut self) {
        if self.plugins.runtime.is_none() {
            return;
        }
        let receivers = self.file_text_receivers();
        self.plugins
            .pushed
            .retain(|id, _| receivers.iter().any(|(running, _)| running == id));
        if receivers.is_empty() {
            return;
        }
        let active = self.state.workspace().active_file();
        for (id, kinds) in receivers {
            let wanted = active.as_ref().filter(|file| has_kind(&file.path, &kinds));
            let (now, message) = match wanted {
                None => (Pushed::Cleared, ToPlugin::FileCleared),
                Some(file) => {
                    let key = file_key(file);
                    if self.plugins.pushed.get(&id) == Some(&Pushed::File(key.clone())) {
                        continue;
                    }
                    let shown = self.shown_path(&file.path);
                    (Pushed::File(key), file_message(file, shown))
                }
            };
            if self.plugins.pushed.get(&id) == Some(&now) {
                continue;
            }
            self.plugins.pushed.insert(id.clone(), now);
            self.send_to_plugin(&id, message);
        }
    }

    /// The plugin is told again about the file in front (it was started or came back).
    pub(super) fn forget_pushed(&mut self, plugin: &str) {
        self.plugins.pushed.remove(plugin);
    }

    /// `path` from the project's folder with `/`, or the whole path when it is outside.
    fn shown_path(&self, path: &Path) -> String {
        let root = self.project_root();
        let relative = path
            .strip_prefix(&root)
            .map(Path::to_path_buf)
            .or_else(|_| {
                canonical(path)
                    .strip_prefix(canonical(&root))
                    .map(Path::to_path_buf)
            });
        let shown = relative.as_deref().unwrap_or(path);
        shown.to_string_lossy().replace('\\', "/")
    }
}

fn file_key(file: &ActiveFile) -> FileKey {
    let metadata = fs::metadata(&file.path).ok();
    FileKey {
        path: file.path.clone(),
        modified: file.modified,
        saves: file.saves,
        reloads: file.reloads,
        changed: metadata.as_ref().and_then(|m| m.modified().ok()),
        len: metadata.map_or(0, |m| m.len()),
    }
}

/// What the plugin is told of `file`: its saved text, that it is too large to send, or
/// (when it cannot be read as text) that there is nothing.
fn file_message(file: &ActiveFile, shown: String) -> ToPlugin {
    let too_large = |bytes: u64| ToPlugin::FileTooLarge {
        path: shown.clone(),
        bytes,
    };
    let Ok(metadata) = fs::metadata(&file.path) else {
        return ToPlugin::FileCleared;
    };
    if metadata.len() > MAX_FILE_TEXT as u64 {
        return too_large(metadata.len());
    }
    let Ok(bytes) = fs::read(&file.path) else {
        return ToPlugin::FileCleared;
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return ToPlugin::FileCleared;
    };
    let message = ToPlugin::FileText {
        path: shown.clone(),
        text,
        modified: file.modified,
    };
    // Written as JSON the text may be longer (a control character is six bytes).
    match message.to_bytes() {
        Ok(_) => message,
        Err(MessageError::TooBig(bytes)) => too_large(bytes as u64),
        Err(_) => ToPlugin::FileCleared,
    }
}

#[cfg(test)]
mod tests;
