//! User configuration files: where they live and how each is loaded. A file
//! that is missing is normal; one that is unreadable or invalid is reported as
//! a [`ConfigWarning`] and the built-in default is used in its place.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

mod files;
mod keymap;
mod layout;
mod settings;
mod theme;
mod warning;

pub(crate) use files::ConfigFile;
pub(crate) use keymap::load_keymap;
pub(crate) use layout::load_layout;
pub(crate) use settings::load_settings;
#[cfg(test)]
pub(crate) use theme::load_theme;
pub(crate) use theme::{
    ThemeChoice, installed_theme_path, installed_themes, load_chosen_theme, resolve_theme,
    startup_theme, theme_file_stem, theme_template, theme_templates, themes_dir,
};
pub(crate) use warning::ConfigWarning;

const CONFIG_DIR: &str = "terminalcode";

/// The directory of the user's files, e.g. `~/.config/terminalcode`.
pub(crate) fn user_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join(CONFIG_DIR))
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
