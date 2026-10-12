//! What is said when a config file could not be used as it is.

use std::error::Error;
use std::path::PathBuf;

use thiserror::Error;

type Source = Box<dyn Error + Send + Sync>;

#[derive(Debug, Error)]
pub(crate) enum ConfigWarning {
    /// The file could not be read or is invalid, so the built-in `what` is used.
    #[error("{}: {source}; using default {what}", path.display())]
    Unusable {
        path: PathBuf,
        source: Source,
        what: &'static str,
    },
    /// A theme file that does not fit over the other theme layers.
    #[error("{}: {source}; leaving that theme out", path.display())]
    LeftOut { path: PathBuf, source: Source },
    #[error("{}: no such setting: {}", path.display(), names.join(", "))]
    NoSuchSettings { path: PathBuf, names: Vec<String> },
    #[error("{}: set more than once, the last one counts: {}", path.display(), names.join(", "))]
    RepeatedSettings { path: PathBuf, names: Vec<String> },
    #[error("settings.json: no theme called \"{0}\"")]
    NoSuchTheme(String),
    #[error("{}", join(.0))]
    Several(Vec<ConfigWarning>),
}

impl ConfigWarning {
    pub(super) fn unusable(
        path: &std::path::Path,
        source: impl Into<Source>,
        what: &'static str,
    ) -> Self {
        Self::Unusable {
            path: path.to_owned(),
            source: source.into(),
            what,
        }
    }

    /// One warning for all of `warnings`; `None` when there are none.
    pub(super) fn all(mut warnings: Vec<Self>) -> Option<Self> {
        match warnings.len() {
            0 => None,
            1 => warnings.pop(),
            _ => Some(Self::Several(warnings)),
        }
    }
}

fn join(warnings: &[ConfigWarning]) -> String {
    let texts: Vec<String> = warnings.iter().map(ToString::to_string).collect();
    texts.join("; ")
}
