//! What is read from the user's files before the editor starts.

use std::path::Path;

use crate::app::state::AppState;
use crate::config::{self, ConfigFile, ConfigWarning, ThemeChoice};
use crate::keymap::Keymap;
use crate::settings::Settings;
use crate::terminal::KeyboardSupport;

/// The parts of the user's configuration the app keeps.
pub(crate) struct UserConfig {
    pub(crate) settings: Settings,
    pub(crate) theme: ThemeChoice,
    pub(crate) keymap: Keymap,
}

/// Reads the settings, theme, layout and keymap files in `config_dir`, puts the
/// theme and layout in `state` and returns the rest. Whatever cannot be used is
/// shown as an error and the built-in default stands in for it.
pub(crate) fn load_user_config(
    state: &mut AppState,
    config_dir: Option<&Path>,
    keyboard: KeyboardSupport,
) -> UserConfig {
    let path = |file: ConfigFile| config_dir.map(|dir| file.path_in(dir));
    let mut report = |warning: Option<ConfigWarning>| {
        if let Some(warning) = warning {
            state.notify_error(warning.to_string());
        }
    };

    let loaded = config::load_settings(path(ConfigFile::Settings).as_deref());
    report(loaded.warning);
    let settings = loaded.settings.unwrap_or_default();

    let (theme, warning) = config::startup_theme(&settings.theme, config_dir);
    report(warning);
    let loaded_theme = config::load_chosen_theme(&theme, config_dir);
    report(loaded_theme.warning);

    let loaded_layout = config::load_layout(path(ConfigFile::Layout).as_deref());
    report(loaded_layout.warning);

    let loaded_keymap = config::load_keymap(path(ConfigFile::Keymap).as_deref(), keyboard);
    report(loaded_keymap.warning);

    state.set_theme(loaded_theme.theme);
    state.set_layout(loaded_layout.layout);
    UserConfig {
        settings,
        theme,
        keymap: loaded_keymap.keymap,
    }
}

#[cfg(test)]
mod tests;
