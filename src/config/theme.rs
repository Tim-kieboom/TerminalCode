use std::fs;
use std::path::{Path, PathBuf};

use super::{ConfigFile, ConfigWarning, read_user_file};
use crate::ui::theme::{Theme, ThemeVariant};

/// A theme plus the reason, if any, that the user's file was not applied.
#[derive(Debug)]
pub(crate) struct LoadedTheme {
    pub(crate) theme: Theme,
    pub(crate) warning: Option<ConfigWarning>,
}

/// Which theme is in use: an installed one by name, or none, which is the
/// fallback (the built-in dark theme, never listed). The built-in themes are
/// only templates for new themes; the fallback is also the floor under every
/// installed theme.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ThemeChoice {
    pub(crate) installed: Option<String>,
}

impl ThemeChoice {
    pub(crate) fn installed(name: impl Into<String>) -> Self {
        Self {
            installed: Some(name.into()),
        }
    }

    /// The name for messages: the installed theme's, else "default".
    pub(crate) fn name(&self) -> &str {
        self.installed.as_deref().unwrap_or("default")
    }

    /// The file of the installed theme, if one is in use.
    pub(crate) fn installed_path(&self, config_dir: Option<&Path>) -> Option<PathBuf> {
        installed_theme_path(config_dir?, self.installed.as_deref()?)
    }
}

/// The theme a `theme` setting names: the installed theme of that name, in the
/// themes folder. `None` when there is no such theme; the built-in themes are
/// not themes to choose, only templates.
pub(crate) fn resolve_theme(name: &str, config_dir: Option<&Path>) -> Option<ThemeChoice> {
    config_dir
        .map(installed_themes)
        .unwrap_or_default()
        .into_iter()
        .find(|installed| installed == name)
        .map(ThemeChoice::installed)
}

/// The theme to start with, from the `theme` setting. No name is the fallback;
/// a name that is no installed theme gives the fallback and a warning.
pub(crate) fn startup_theme(
    name: Option<&str>,
    config_dir: Option<&Path>,
) -> (ThemeChoice, Option<ConfigWarning>) {
    let Some(name) = name else {
        return (ThemeChoice::default(), None);
    };
    match resolve_theme(name, config_dir) {
        Some(choice) => (choice, None),
        None => (
            ThemeChoice::default(),
            Some(ConfigWarning::NoSuchTheme(name.to_owned())),
        ),
    }
}

/// The fallback (built-in dark) theme with the user's file laid over it (see
/// [`Theme::layered`]). A missing file is normal; an unreadable or invalid one
/// (a syntax error, a bad color, an unknown `@name`) is reported and the whole
/// built-in theme is used, not a half-applied one. The built-in theme itself
/// is not allowed to fail: it panics.
#[cfg(test)]
pub(crate) fn load_theme(user_file: Option<&Path>) -> LoadedTheme {
    load_over_fallback(user_file)
}

/// The fallback theme with the user's `theme.toml` over it, as [`load_theme`].
fn load_over_fallback(user_file: Option<&Path>) -> LoadedTheme {
    let variant = ThemeVariant::Dark;
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
        Err(error) => return fallback(Some(ConfigWarning::unusable(path, error, "theme"))),
    };

    match Theme::layered_over(variant, &source) {
        Ok(theme) => LoadedTheme {
            theme,
            warning: None,
        },
        Err(error) => fallback(Some(ConfigWarning::unusable(path, error, "theme"))),
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
/// installed theme if there is one (it may leave slots out), then the user's
/// own `theme.toml` over both. Anything unreadable or invalid is reported and
/// left out, as for [`load_theme`].
pub(crate) fn load_chosen_theme(choice: &ThemeChoice, config_dir: Option<&Path>) -> LoadedTheme {
    let user_file = config_dir.map(|dir| ConfigFile::Theme.path_in(dir));
    let Some(installed) = choice.installed_path(config_dir) else {
        return load_over_fallback(user_file.as_deref());
    };
    let mut warnings = Vec::new();
    let mut layers = vec![ThemeVariant::Dark.source().to_owned()];
    // An unchanged copy of a built-in theme is the theme itself when it is the
    // installed one, but nothing when it is the overlay (it would undo the choice).
    for (path, is_overlay) in [
        (Some(installed.as_path()), false),
        (user_file.as_deref(), true),
    ] {
        let Some(path) = path else { continue };
        match read_user_file(path) {
            Ok(Some(source)) if is_overlay && is_stock_copy(&source) => {}
            Ok(Some(source)) => {
                let mut trial: Vec<&str> = layers.iter().map(String::as_str).collect();
                trial.push(&source);
                match Theme::layered_many(&trial) {
                    Ok(_) => layers.push(source),
                    Err(error) => warnings.push(left_out(path, error)),
                }
            }
            Ok(None) => {}
            Err(error) => warnings.push(left_out(path, error)),
        }
    }
    let sources: Vec<&str> = layers.iter().map(String::as_str).collect();
    LoadedTheme {
        theme: Theme::layered_many(&sources).unwrap_or_default(),
        warning: ConfigWarning::all(warnings),
    }
}

fn left_out(path: &Path, error: impl std::error::Error + Send + Sync + 'static) -> ConfigWarning {
    ConfigWarning::LeftOut {
        path: path.to_owned(),
        source: Box::new(error),
    }
}

/// Whether a theme file is only a copy of a built-in theme, as the file made
/// by older versions of "Config: Open Theme" was. Laid over the chosen theme it
/// would give every slot back its value in the built-in theme and so undo the
/// choice, so it counts as no overlay at all.
fn is_stock_copy(source: &str) -> bool {
    ThemeVariant::ALL
        .iter()
        .any(|variant| variant.source().trim() == source.trim())
}

/// What a new theme starts as: the text of the template called `name`, which is
/// `Dark` or `Light` for the built-in themes, else an installed theme.
pub(crate) fn theme_template(config_dir: &Path, name: &str) -> Option<String> {
    match ThemeVariant::ALL
        .iter()
        .find(|variant| variant.title() == name)
    {
        Some(variant) => Some(variant.source().to_owned()),
        None => fs::read_to_string(installed_theme_path(config_dir, name)?).ok(),
    }
}

/// The names to offer as templates for a new theme: the built-in themes, then the
/// installed ones (leaving out the copies of the built-in ones the editor made,
/// which are the same as the built-in ones until edited).
pub(crate) fn theme_templates(config_dir: Option<&Path>) -> Vec<String> {
    let mut names: Vec<String> = ThemeVariant::ALL
        .iter()
        .map(|variant| variant.title().to_owned())
        .collect();
    if let Some(dir) = config_dir {
        names.extend(installed_themes(dir).into_iter().filter(|name| {
            let lower = name.to_lowercase();
            !ThemeVariant::ALL
                .iter()
                .any(|variant| variant.name() == lower)
        }));
    }
    names
}

/// A theme name as a file stem: trimmed, without a trailing `.toml`. `None` for
/// an empty name, one with a slash, or one that starts with a dot.
pub(crate) fn theme_file_stem(name: &str) -> Option<String> {
    let name = name.trim();
    let name = name.strip_suffix(".toml").unwrap_or(name).trim();
    let usable = !name.is_empty() && !name.contains(['/', '\\']) && !name.starts_with('.');
    usable.then(|| name.to_owned())
}
