//! The user's `settings.json`: JSON with comments and trailing commas.
//!
//! Every setting has a default, so the file only needs the ones that differ.
//! A setting with the wrong type makes the whole file unusable (it is reported
//! and the defaults are used); a setting that does not exist is reported but
//! the rest of the file is still applied.

use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use thiserror::Error;

pub(crate) const DEFAULT_SETTINGS_JSONC: &str = include_str!("../../defaults/settings.jsonc");

/// The settings this version knows.
const KNOWN: [&str; 1] = ["theme"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Settings {
    /// The theme to start with: a built-in theme or an installed one, by name.
    pub(crate) theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".to_owned(),
        }
    }
}

/// Settings read from a file, with the names in it that are not settings.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Parsed {
    pub(crate) settings: Settings,
    pub(crate) unknown: Vec<String>,
}

#[derive(Debug, Error)]
pub(crate) enum SettingsError {
    #[error("{0}")]
    Syntax(#[from] serde_json::Error),
    #[error("the file must hold one object, {{ ... }}")]
    NotAnObject,
    #[error("setting \"{key}\": {source}")]
    Value {
        key: &'static str,
        source: serde_json::Error,
    },
}

impl Settings {
    /// Reads settings from the text of a settings file. Settings left out keep
    /// their defaults, and a file with nothing in it is all defaults.
    pub(crate) fn parse(source: &str) -> Result<Parsed, SettingsError> {
        let cleaned = strip_jsonc(source);
        if cleaned.trim().is_empty() {
            return Ok(Parsed {
                settings: Self::default(),
                unknown: Vec::new(),
            });
        }
        let Value::Object(mut map) = serde_json::from_str(&cleaned)? else {
            return Err(SettingsError::NotAnObject);
        };

        let unknown = map
            .keys()
            .filter(|key| !KNOWN.contains(&key.as_str()))
            .cloned()
            .collect();
        let defaults = Self::default();
        let settings = Self {
            theme: take(&mut map, "theme")?.unwrap_or(defaults.theme),
        };
        Ok(Parsed { settings, unknown })
    }
}

/// The setting `key`, if the file has it.
fn take<T: DeserializeOwned>(
    map: &mut Map<String, Value>,
    key: &'static str,
) -> Result<Option<T>, SettingsError> {
    map.remove(key)
        .map(|value| {
            serde_json::from_value(value).map_err(|source| SettingsError::Value { key, source })
        })
        .transpose()
}

/// `source` with the comments and the commas before a closing bracket made
/// into spaces, so what is left is plain JSON whose line and column numbers
/// still match the file.
pub(crate) fn strip_jsonc(source: &str) -> String {
    let mut out: Vec<char> = Vec::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            match c {
                '\\' => out.extend(chars.next()),
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                out.push(c);
            }
            ('/', Some('/')) => {
                out.push(' ');
                while let Some(next) = chars.next_if(|next| *next != '\n') {
                    out.push(blank(next));
                }
            }
            ('/', Some('*')) => {
                out.extend([' ', ' ']);
                chars.next();
                let mut last = ' ';
                for next in chars.by_ref() {
                    out.push(blank(next));
                    if last == '*' && next == '/' {
                        break;
                    }
                    last = next;
                }
            }
            _ => out.push(c),
        }
    }
    blank_trailing_commas(&mut out);
    out.into_iter().collect()
}

/// A space for anything but a line break, which has to stay to keep the line numbers.
fn blank(c: char) -> char {
    if c == '\n' { c } else { ' ' }
}

/// Turns each comma that only has whitespace between it and a `}` or `]` into a space.
fn blank_trailing_commas(chars: &mut [char]) {
    let mut in_string = false;
    let mut escaped = false;
    for index in 0..chars.len() {
        let c = chars[index];
        if in_string {
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            ',' => {
                let next = chars[index + 1..].iter().find(|c| !c.is_whitespace());
                if matches!(next, Some('}' | ']')) {
                    chars[index] = ' ';
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests;
