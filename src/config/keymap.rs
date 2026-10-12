use std::path::Path;

use super::{ConfigWarning, read_user_file};
use crate::keymap::Keymap;
use crate::terminal::KeyboardSupport;

/// A keymap plus the reason, if any, that the user's file was not applied.
#[derive(Debug)]
pub(crate) struct LoadedKeymap {
    pub(crate) keymap: Keymap,
    pub(crate) warning: Option<ConfigWarning>,
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

    let source = match read_user_file(path) {
        Ok(Some(source)) => source,
        Ok(None) => {
            return LoadedKeymap {
                keymap,
                warning: None,
            };
        }
        Err(error) => return unusable(keymap, path, error),
    };

    // `apply_toml` leaves the keymap untouched when the file is invalid.
    match keymap.apply_toml(&source) {
        Ok(()) => LoadedKeymap {
            keymap,
            warning: None,
        },
        Err(error) => unusable(keymap, path, error),
    }
}

fn unusable(
    keymap: Keymap,
    path: &Path,
    error: impl std::error::Error + Send + Sync + 'static,
) -> LoadedKeymap {
    LoadedKeymap {
        keymap,
        warning: Some(ConfigWarning::unusable(path, error, "keybindings")),
    }
}
