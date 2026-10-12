//! The user's `settings.json`: JSON with comments and trailing commas.
//!
//! Every setting has a default, so the file only needs the ones that differ.
//! A setting with the wrong type makes the whole file unusable (it is reported
//! and the defaults are used); a setting that does not exist, or is set twice,
//! is reported but the rest of the file is still applied.

use std::fmt;

use serde::de::{DeserializeOwned, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};
use thiserror::Error;

pub(crate) const DEFAULT_SETTINGS_JSONC: &str = include_str!("../../defaults/settings.jsonc");

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

/// Settings read from a file, with the names in it that need a remark.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Parsed {
    pub(crate) settings: Settings,
    /// Names that are not settings.
    pub(crate) unknown: Vec<String>,
    /// Names that are set more than once; the last one counts.
    pub(crate) repeated: Vec<String>,
}

#[derive(Debug, Error)]
pub(crate) enum SettingsError {
    /// Bad syntax, or something other than one object, `{ ... }`.
    #[error("{0}")]
    Syntax(#[from] serde_json::Error),
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
                repeated: Vec::new(),
            });
        }

        let Pairs(pairs) = serde_json::from_str(&cleaned)?;
        let mut map = Map::new();
        let mut repeated: Vec<String> = Vec::new();
        for (key, value) in pairs {
            let again = map.insert(key.clone(), value).is_some();
            if again && !repeated.contains(&key) {
                repeated.push(key);
            }
        }

        let defaults = Self::default();
        let settings = Self {
            theme: take(&mut map, "theme")?.unwrap_or(defaults.theme),
        };
        // Every setting that is known has been taken out, so what is left is not one.
        let unknown = map.into_iter().map(|(key, _)| key).collect();
        Ok(Parsed {
            settings,
            unknown,
            repeated,
        })
    }
}

/// The members of a JSON object in file order, repeats included (a map would
/// quietly keep the last of a repeated name).
struct Pairs(Vec<(String, Value)>);

impl<'de> Deserialize<'de> for Pairs {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Members;

        impl<'de> Visitor<'de> for Members {
            type Value = Pairs;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("one object, { ... }")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Pairs, A::Error> {
                let mut pairs = Vec::new();
                while let Some(pair) = access.next_entry::<String, Value>()? {
                    pairs.push(pair);
                }
                Ok(Pairs(pairs))
            }
        }

        deserializer.deserialize_map(Members)
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
fn strip_jsonc(source: &str) -> String {
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
