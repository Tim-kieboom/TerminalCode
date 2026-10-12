use std::path::Path;

use super::{ConfigWarning, read_user_file};
use crate::settings::Settings;

/// The settings plus what was wrong with the user's file, if anything.
#[derive(Debug)]
pub(crate) struct LoadedSettings {
    /// What to apply; `None` when the file is unusable, so a reload can leave
    /// what is applied alone instead of going back to the defaults.
    pub(crate) settings: Option<Settings>,
    pub(crate) warning: Option<ConfigWarning>,
}

/// The settings in the user's file. A missing file is normal and means the
/// defaults. An unreadable or invalid one is reported and gives no settings; a
/// setting that does not exist, or is set twice, is reported and the others
/// are applied.
pub(crate) fn load_settings(user_file: Option<&Path>) -> LoadedSettings {
    let defaults = LoadedSettings {
        settings: Some(Settings::default()),
        warning: None,
    };
    let Some(path) = user_file else {
        return defaults;
    };
    let unusable = |error: Box<dyn std::error::Error + Send + Sync>| LoadedSettings {
        settings: None,
        warning: Some(ConfigWarning::unusable(path, error, "settings")),
    };

    let source = match read_user_file(path) {
        Ok(Some(source)) => source,
        Ok(None) => return defaults,
        Err(error) => return unusable(Box::new(error)),
    };
    let parsed = match Settings::parse(&source) {
        Ok(parsed) => parsed,
        Err(error) => return unusable(Box::new(error)),
    };

    let mut warnings = Vec::new();
    if !parsed.unknown.is_empty() {
        warnings.push(ConfigWarning::NoSuchSettings {
            path: path.to_owned(),
            names: parsed.unknown,
        });
    }
    if !parsed.repeated.is_empty() {
        warnings.push(ConfigWarning::RepeatedSettings {
            path: path.to_owned(),
            names: parsed.repeated,
        });
    }
    LoadedSettings {
        settings: Some(parsed.settings),
        warning: ConfigWarning::all(warnings),
    }
}
