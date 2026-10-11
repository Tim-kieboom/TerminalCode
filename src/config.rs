//! User configuration files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::keymap::{DEFAULT_KEYMAP_TOML, Keymap};
use crate::terminal::KeyboardSupport;
use crate::ui::layout::{DEFAULT_LAYOUT_RON, LayoutTree};
use crate::ui::theme::{DEFAULT_THEME_TOML, Theme, ThemeVariant};

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
    load_theme_for(ThemeVariant::Dark, user_file)
}

/// Like [`load_theme`], over the built-in theme `variant` instead of the dark one.
pub(crate) fn load_theme_for(variant: ThemeVariant, user_file: Option<&Path>) -> LoadedTheme {
    let fallback = |warning| LoadedTheme {
        theme: Theme::built_in(variant),
        warning,
    };
    let Some(path) = user_file else {
        return fallback(None);
    };

    let source = match read_user_file(path) {
        Ok(Some(source)) if !is_stock_copy(&source) => source,
        Ok(_) => return fallback(None),
        Err(error) => return fallback(Some(unusable_theme(path, &error))),
    };

    match Theme::layered_over(variant, &source) {
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
    /// Every file, for looking through them.
    pub(crate) const ALL: [Self; 3] = [Self::Theme, Self::Keymap, Self::Layout];

    fn file_name(self) -> &'static str {
        match self {
            Self::Theme => THEME_FILE,
            Self::Keymap => KEYMAP_FILE,
            Self::Layout => LAYOUT_FILE,
        }
    }

    /// The text of the built-in default, which a new file starts from.
    pub(crate) fn default_content(self) -> &'static str {
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

/// The folder of installed themes, `themes` in the config folder.
pub(crate) fn themes_dir(config_dir: &Path) -> PathBuf {
    config_dir.join("themes")
}

/// The names of the installed themes (the files `NAME.toml` in the themes
/// folder), in name order. None when the folder is missing or unreadable.
pub(crate) fn installed_themes(config_dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(themes_dir(config_dir)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "toml"))
        .filter_map(|path| Some(path.file_stem()?.to_str()?.to_owned()))
        .collect();
    names.sort();
    names
}

/// The file of the installed theme `name`; `None` for a name that is not a
/// plain file name, which could point outside the folder.
pub(crate) fn installed_theme_path(config_dir: &Path, name: &str) -> Option<PathBuf> {
    let plain = !name.is_empty() && !name.contains(['/', '\\']) && name != "." && name != "..";
    plain.then(|| themes_dir(config_dir).join(format!("{name}.toml")))
}

/// The theme the user chose: the built-in dark theme as the floor, then the
/// installed theme `installed` if there is one (it may leave slots out), then
/// the user's own `theme.toml` over both. Anything unreadable or invalid is
/// reported and left out, as for [`load_theme_for`].
pub(crate) fn load_chosen_theme(
    variant: ThemeVariant,
    installed: Option<&Path>,
    user_file: Option<&Path>,
) -> LoadedTheme {
    let Some(installed) = installed else {
        return load_theme_for(variant, user_file);
    };
    let floor = ThemeVariant::Dark.source();
    let mut warnings = Vec::new();
    let mut layers = vec![floor.to_owned()];
    for (path, kind) in [(Some(installed), "theme"), (user_file, "theme")] {
        let Some(path) = path else { continue };
        match read_user_file(path) {
            Ok(Some(source)) if is_stock_copy(&source) => {}
            Ok(Some(source)) => {
                let mut trial: Vec<&str> = layers.iter().map(String::as_str).collect();
                trial.push(&source);
                match Theme::layered_many(&trial) {
                    Ok(_) => layers.push(source),
                    Err(error) => warnings.push(unusable_layer(path, kind, &error)),
                }
            }
            Ok(None) => {}
            Err(error) => warnings.push(unusable_layer(path, kind, &error)),
        }
    }
    let sources: Vec<&str> = layers.iter().map(String::as_str).collect();
    LoadedTheme {
        theme: Theme::layered_many(&sources).unwrap_or_default(),
        warning: (!warnings.is_empty()).then(|| warnings.join("; ")),
    }
}

fn unusable_layer(path: &Path, kind: &str, error: &dyn std::fmt::Display) -> String {
    format!("{}: {error}; leaving that {kind} out", path.display())
}

/// Whether a theme file is only a copy of a built-in theme, as the file made
/// by older versions of "Config: Open Theme" was. Laid over the chosen theme it
/// would give every slot back its value in the built-in theme and so undo the
/// choice, so it counts as no overlay at all.
fn is_stock_copy(source: &str) -> bool {
    [ThemeVariant::Dark, ThemeVariant::Light]
        .iter()
        .any(|variant| variant.source().trim() == source.trim())
}

/// What a new `theme.toml` starts with: nothing that would hide the theme in
/// use, and a hint at what to write.
pub(crate) const THEME_OVERLAY_TEMPLATE: &str = "\
# Your changes to the theme in use (choose one with \"Theme: Choose...\").
# Whatever is written here is laid over that theme, so only write what differs.
# Slots and colors are described at the top of the built-in themes. For example:
#
# [palette]
# accent = \"#00ff00\"
#
# [\"editor.selection\"]
# background = \"#334455\"
";
