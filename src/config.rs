//! User configuration files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::keymap::Keymap;
use crate::terminal::KeyboardSupport;
use crate::ui::layout::LayoutTree;

const CONFIG_DIR: &str = "terminalcode";
const KEYMAP_FILE: &str = "keymap.toml";
const LAYOUT_FILE: &str = "layout.ron";

/// A keymap plus the reason, if any, that the user's file was not applied.
#[derive(Debug)]
pub(crate) struct LoadedKeymap {
    pub(crate) keymap: Keymap,
    pub(crate) warning: Option<String>,
}

/// Where the user's keymap lives, e.g. `~/.config/terminalcode/keymap.toml`.
pub(crate) fn user_keymap_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join(CONFIG_DIR).join(KEYMAP_FILE))
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
    dirs::config_dir().map(|dir| dir.join(CONFIG_DIR).join(LAYOUT_FILE))
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

    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return fallback(None),
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
