use std::collections::HashMap;
use std::str::FromStr;

use serde::Deserialize;
use thiserror::Error;

use crate::action::Action;

pub(crate) use key::KeyChord;
use key::KeyParseError;

mod key;
#[cfg(test)]
mod tests;

/// Built-in bindings, embedded at compile time.
const DEFAULT_KEYMAP_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/defaults/default_keymap.toml"
));

#[derive(Debug, Error)]
pub enum KeymapError {
    #[error("keymap is not valid TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("binding `{keys}`: {source}")]
    InvalidKey {
        keys: Box<str>,
        source: KeyParseError,
    },
    #[error("`{0}` is bound more than once")]
    DuplicateBinding(Box<str>),
}

#[derive(Debug, Deserialize)]
struct KeymapFile {
    #[serde(default)]
    binding: Vec<BindingSpec>,
}

#[derive(Debug, Deserialize)]
struct BindingSpec {
    keys: Box<str>,
    action: Action,
}

/// Maps key chords to actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Keymap {
    bindings: HashMap<KeyChord, Action>,
}

impl Keymap {
    /// Parses a keymap described as TOML:
    ///
    /// ```toml
    /// [[binding]]
    /// keys = "ctrl+q"
    /// action = "quit"
    /// ```
    pub(crate) fn from_toml(source: &str) -> Result<Self, KeymapError> {
        let file: KeymapFile = toml::from_str(source)?;
        let mut bindings = HashMap::with_capacity(file.binding.len());
        for spec in file.binding {
            let chord =
                KeyChord::from_str(&spec.keys).map_err(|source| KeymapError::InvalidKey {
                    keys: spec.keys.clone(),
                    source,
                })?;
            if bindings.insert(chord, spec.action).is_some() {
                return Err(KeymapError::DuplicateBinding(spec.keys));
            }
        }
        Ok(Self { bindings })
    }

    pub(crate) fn lookup(&self, chord: &KeyChord) -> Option<&Action> {
        self.bindings.get(chord)
    }
}

impl Default for Keymap {
    fn default() -> Self {
        Self::from_toml(DEFAULT_KEYMAP_TOML)
            .expect("defaults/default_keymap.toml must be a valid keymap")
    }
}
