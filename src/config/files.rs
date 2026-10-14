use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::keymap::DEFAULT_KEYMAP_TOML;
use crate::settings::DEFAULT_SETTINGS_JSONC;
use crate::ui::layout::DEFAULT_LAYOUT_RON;
use crate::ui::theme::DEFAULT_THEME_TOML;

const KEYMAP_FILE: &str = "keymap.toml";
const LAYOUT_FILE: &str = "layout.ron";
const THEME_FILE: &str = "theme.toml";
const SETTINGS_FILE: &str = "settings.json";

/// One of the user's files in the config directory. Each has a built-in
/// default that it is layered over (theme, keymap) or replaces (layout).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConfigFile {
    Theme,
    Keymap,
    Layout,
    Settings,
}

impl ConfigFile {
    /// Every file, for looking through them.
    pub(crate) const ALL: [Self; 4] = [Self::Theme, Self::Keymap, Self::Layout, Self::Settings];

    /// The name of the file inside the config directory.
    pub(crate) fn file_name(self) -> &'static str {
        match self {
            Self::Theme => THEME_FILE,
            Self::Keymap => KEYMAP_FILE,
            Self::Layout => LAYOUT_FILE,
            Self::Settings => SETTINGS_FILE,
        }
    }

    /// The text of the built-in default, which a new file starts from.
    pub(crate) fn default_content(self) -> &'static str {
        match self {
            Self::Theme => DEFAULT_THEME_TOML,
            Self::Keymap => DEFAULT_KEYMAP_TOML,
            Self::Layout => DEFAULT_LAYOUT_RON,
            Self::Settings => DEFAULT_SETTINGS_JSONC,
        }
    }

    /// Where the file is inside the user's config directory.
    pub(crate) fn path_in(self, config_dir: &Path) -> PathBuf {
        config_dir.join(self.file_name())
    }

    /// Writes the built-in default to `path` if there is no file there yet,
    /// making the directory too; an existing file is never touched. Returns
    /// whether the file was created.
    pub(crate) fn create_from_default(self, path: &Path) -> io::Result<bool> {
        self.create_with(path, self.default_content())
    }

    /// Like [`ConfigFile::create_from_default`], starting the new file from `content`.
    pub(crate) fn create_with(self, path: &Path, content: &str) -> io::Result<bool> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let created = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path);

        match created {
            Ok(mut file) => {
                io::Write::write_all(&mut file, content.as_bytes())?;
                Ok(true)
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(false),
            Err(error) => Err(error),
        }
    }
}

/// Where the backup of the file at `path` goes: beside it, with `.bak` added.
pub(crate) fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

/// Puts `default` in the place of the file at `path`, keeping what was there as
/// its `.bak` (an older backup is replaced). Returns the backup, when there was a
/// file to keep. If the old file cannot be moved nothing is touched; if the
/// default cannot be written, the old file is put back.
pub(crate) fn reset_with_backup(path: &Path, default: &str) -> io::Result<Option<PathBuf>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let backup = backup_path(path);
    let kept = match fs::symlink_metadata(path) {
        Ok(_) => {
            fs::rename(path, &backup)?;
            Some(backup.clone())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    if let Err(error) = fs::write(path, default) {
        if kept.is_some() {
            let _ = fs::rename(&backup, path);
        }
        return Err(error);
    }
    Ok(kept)
}
