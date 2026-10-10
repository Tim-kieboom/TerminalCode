//! User configuration files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::keymap::{DEFAULT_KEYMAP_TOML, Keymap};
use crate::terminal::KeyboardSupport;
use crate::ui::layout::{DEFAULT_LAYOUT_RON, LayoutTree};
use crate::ui::theme::{DEFAULT_THEME_TOML, Theme};

const CONFIG_DIR: &str = "terminalcode";
const KEYMAP_FILE: &str = "keymap.toml";
const LAYOUT_FILE: &str = "layout.ron";
const THEME_FILE: &str = "theme.toml";

/// A keymap plus the reason, if any, that the user's file was not applied.
#[derive(Debug)]
pub(crate) struct LoadedKeymap {
    pub(crate) keymap: Keymap,
    pub(crate) warning: Option<String>,
}

/// The directory of the user's files, e.g. `~/.config/terminalcode`.
pub(crate) fn user_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join(CONFIG_DIR))
}

/// Where the user's keymap lives, e.g. `~/.config/terminalcode/keymap.toml`.
pub(crate) fn user_keymap_path() -> Option<PathBuf> {
    user_config_dir().map(|dir| ConfigFile::Keymap.path_in(&dir))
}

/// Default bindings with the user's file layered on top. A missing file is
/// normal; an unreadable or invalid one is reported and the defaults are used.
pub(crate) fn load_keymap(user_file: Option<&Path>, keyboard: KeyboardSupport) -> LoadedKeymap {
    let mut keymap = Keymap::defaults(keyboard);
    let Some(path) = user_file else {
        return LoadedKeymap {
            keymap,
            warning: None,
        };
    };

    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return LoadedKeymap {
                keymap,
                warning: None,
            };
        }
        Err(error) => return unusable(keymap, path, &error),
    };

    // `apply_toml` leaves the keymap untouched when the file is invalid.
    match keymap.apply_toml(&source) {
        Ok(()) => LoadedKeymap {
            keymap,
            warning: None,
        },
        Err(error) => unusable(keymap, path, &error),
    }
}

fn unusable(keymap: Keymap, path: &Path, error: &dyn std::fmt::Display) -> LoadedKeymap {
    LoadedKeymap {
        keymap,
        warning: Some(format!(
            "{}: {error}; using default keybindings",
            path.display()
        )),
    }
}

/// A layout plus the reason, if any, that the user's file was not applied.
#[derive(Debug)]
pub(crate) struct LoadedLayout {
    pub(crate) layout: LayoutTree,
    pub(crate) warning: Option<String>,
}

/// Where the user's layout lives, e.g. `~/.config/terminalcode/layout.ron`.
pub(crate) fn user_layout_path() -> Option<PathBuf> {
    user_config_dir().map(|dir| ConfigFile::Layout.path_in(&dir))
}

/// The user's layout file if there is a valid one, else the built-in layout.
/// A missing file is normal; an unreadable or invalid one is reported (with
/// the place in the tree for a layout error) and the built-in layout is used.
/// The built-in layout itself is not allowed to fail: it panics.
pub(crate) fn load_layout(user_file: Option<&Path>) -> LoadedLayout {
    let fallback = |warning| LoadedLayout {
        layout: LayoutTree::default(),
        warning,
    };
    let Some(path) = user_file else {
        return fallback(None);
    };

    let source = match read_user_file(path) {
        Ok(Some(source)) => source,
        Ok(None) => return fallback(None),
        Err(error) => return fallback(Some(unusable_layout(path, &error))),
    };

    match LayoutTree::from_ron(&source) {
        Ok(layout) => LoadedLayout {
            layout,
            warning: None,
        },
        Err(error) => fallback(Some(unusable_layout(path, &error))),
    }
}

fn unusable_layout(path: &Path, error: &dyn std::fmt::Display) -> String {
    format!("{}: {error}; using default layout", path.display())
}

/// The text of a user file, or `None` when there is no such file, which is
/// normal. Any other failure to read it is an error.
fn read_user_file(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(source) => Ok(Some(source)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// A theme plus the reason, if any, that the user's file was not applied.
#[derive(Debug)]
pub(crate) struct LoadedTheme {
    pub(crate) theme: Theme,
    pub(crate) warning: Option<String>,
}

/// Where the user's theme lives, e.g. `~/.config/terminalcode/theme.toml`.
pub(crate) fn user_theme_path() -> Option<PathBuf> {
    user_config_dir().map(|dir| ConfigFile::Theme.path_in(&dir))
}

/// The built-in theme with the user's file laid over it (see
/// [`Theme::layered`]). A missing file is normal; an unreadable or invalid one
/// (a syntax error, a bad color, an unknown `@name`) is reported and the whole
/// built-in theme is used, not a half-applied one. The built-in theme itself
/// is not allowed to fail: it panics.
pub(crate) fn load_theme(user_file: Option<&Path>) -> LoadedTheme {
    let fallback = |warning| LoadedTheme {
        theme: Theme::default(),
        warning,
    };
    let Some(path) = user_file else {
        return fallback(None);
    };

    let source = match read_user_file(path) {
        Ok(Some(source)) => source,
        Ok(None) => return fallback(None),
        Err(error) => return fallback(Some(unusable_theme(path, &error))),
    };

    match Theme::layered_over_default(&source) {
        Ok(theme) => LoadedTheme {
            theme,
            warning: None,
        },
        Err(error) => fallback(Some(unusable_theme(path, &error))),
    }
}

fn unusable_theme(path: &Path, error: &dyn std::fmt::Display) -> String {
    format!("{}: {error}; using default theme", path.display())
}

/// One of the user's files in the config directory. Each has a built-in
/// default that it is layered over (theme, keymap) or replaces (layout).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConfigFile {
    Theme,
    Keymap,
    Layout,
}

impl ConfigFile {
    fn file_name(self) -> &'static str {
        match self {
            Self::Theme => THEME_FILE,
            Self::Keymap => KEYMAP_FILE,
            Self::Layout => LAYOUT_FILE,
        }
    }

    /// The text of the built-in default, which a new file starts from.
    fn default_content(self) -> &'static str {
        match self {
            Self::Theme => DEFAULT_THEME_TOML,
            Self::Keymap => DEFAULT_KEYMAP_TOML,
            Self::Layout => DEFAULT_LAYOUT_RON,
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
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let created = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path);
        match created {
            Ok(mut file) => {
                io::Write::write_all(&mut file, self.default_content().as_bytes())?;
                Ok(true)
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(false),
            Err(error) => Err(error),
        }
    }
}
