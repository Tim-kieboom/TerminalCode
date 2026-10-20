//! The user's `settings.json`: JSON with comments and trailing commas.
//!
//! Every setting has a default, so the file only needs the ones that differ.
//! A setting with the wrong type makes the whole file unusable (it is reported
//! and the defaults are used); a setting that does not exist, or is set twice,
//! is reported but the rest of the file is still applied.

use std::fmt;
use std::time::Duration;

use serde::de::{DeserializeOwned, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};
use thiserror::Error;

pub(crate) mod update;

use crate::components::editor::{IndentKind, IndentPrefs, TabWidth};
use crate::lsp::{ConfigError, LanguageServers};

pub(crate) const DEFAULT_SETTINGS_JSONC: &str = include_str!("../../defaults/settings.jsonc");

/// How long a half-typed key sequence waits when the setting says nothing.
const SEQUENCE_TIMEOUT_DEFAULT: Duration = Duration::from_millis(1000);
/// How long a plain message stays when the setting says nothing.
const NOTIFICATION_LIFETIME_DEFAULT: Duration = crate::components::notifications::INFO_LIFETIME;
/// The seconds `notification_seconds` may be.
const NOTIFICATION_SECONDS_RANGE: std::ops::RangeInclusive<usize> = 1..=60;
/// The milliseconds `sequence_timeout` may be, besides 0 (wait for ever).
const SEQUENCE_TIMEOUT_RANGE: std::ops::RangeInclusive<usize> = 200..=5000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Settings {
    /// The theme to start with: an installed one, by name (a file in the themes
    /// folder). `None` is the fallback theme, which is not listed anywhere.
    pub(crate) theme: Option<String>,
    /// The program the terminal pane runs; `None` is the user's own shell
    /// (`$SHELL`, or the platform's). New terminal tabs use it.
    pub(crate) shell: Option<String>,
    /// Whether the mouse is on at the start. `alt+m` still switches it for the run.
    pub(crate) mouse: bool,
    /// How many columns a tab character draws, and how many a level of
    /// indentation made of spaces is, for a file that shows no style of its own.
    pub(crate) tab_size: TabWidth,
    /// What a level of indentation is made of, for a file that shows no style.
    pub(crate) indent_style: IndentKind,
    /// Whether the indentation a file already has wins over the two settings above.
    pub(crate) detect_indent: bool,
    /// How long a half-typed key sequence (`ctrl+k ctrl+i`) waits for its next chord;
    /// `None` waits for ever. When it runs out the sequence is dropped.
    pub(crate) sequence_timeout: Option<Duration>,
    /// How long a plain message stays on screen. Errors stay until dismissed.
    pub(crate) notification_lifetime: Duration,
    /// Whether the explorer leaves out the entries the ignore rules leave out,
    /// instead of listing them dimmed.
    pub(crate) hide_ignored: bool,
    /// Extra plugin indexes, by URL (an `index.json` like the official one). Their
    /// plugins are listed in the plugin manager beside the official ones, marked with
    /// where they come from.
    pub(crate) plugin_sources: Vec<String>,
    /// The language servers: the built-in ones with the `language_servers` setting
    /// laid over them.
    #[allow(
        dead_code,
        reason = "the app starts servers in step 3 of the LSP client"
    )]
    pub(crate) language_servers: LanguageServers,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: None,
            shell: None,
            mouse: true,
            tab_size: TabWidth::default(),
            indent_style: IndentKind::Spaces,
            detect_indent: true,
            sequence_timeout: Some(SEQUENCE_TIMEOUT_DEFAULT),
            notification_lifetime: NOTIFICATION_LIFETIME_DEFAULT,
            hide_ignored: false,
            plugin_sources: Vec::new(),
            language_servers: default_language_servers().clone(),
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
    #[error("setting {0}")]
    LanguageServers(#[from] ConfigError),
    #[error("setting \"{key}\": {value} is not between {min} and {max}")]
    OutOfRange {
        key: &'static str,
        value: i64,
        min: usize,
        max: usize,
    },
}

impl Settings {
    /// What the indentation settings say, as the editor uses them.
    pub(crate) fn indent_prefs(&self) -> IndentPrefs {
        IndentPrefs {
            kind: self.indent_style,
            size: self.tab_size,
            detect: self.detect_indent,
        }
    }

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
            theme: take::<Option<String>>(&mut map, "theme")?
                .flatten()
                .map(|name| name.trim().to_owned())
                .filter(|name| !name.is_empty()),
            shell: take::<Option<String>>(&mut map, "shell")?
                .flatten()
                .map(|shell| shell.trim().to_owned())
                .filter(|shell| !shell.is_empty()),
            mouse: take(&mut map, "mouse")?.unwrap_or(defaults.mouse),
            tab_size: take_in_range(&mut map, "tab_size", TabWidth::RANGE)?
                .and_then(TabWidth::new)
                .unwrap_or(defaults.tab_size),
            indent_style: take(&mut map, "indent_style")?.unwrap_or(defaults.indent_style),
            detect_indent: take(&mut map, "detect_indent")?.unwrap_or(defaults.detect_indent),
            sequence_timeout: take_sequence_timeout(&mut map)?.unwrap_or(defaults.sequence_timeout),
            notification_lifetime: take_in_range(
                &mut map,
                "notification_seconds",
                NOTIFICATION_SECONDS_RANGE,
            )?
            .map_or(defaults.notification_lifetime, |seconds| {
                Duration::from_secs(seconds as u64)
            }),
            hide_ignored: take(&mut map, "hide_ignored")?.unwrap_or(defaults.hide_ignored),
            plugin_sources: plugin_sources(take::<Vec<String>>(&mut map, "plugin_sources")?),
            language_servers: match map.remove("language_servers") {
                Some(setting) => LanguageServers::layered(default_servers_setting(), &setting)?,
                None => defaults.language_servers,
            },
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

/// The URLs of `plugin_sources`: trimmed, none empty, each once.
fn plugin_sources(listed: Option<Vec<String>>) -> Vec<String> {
    let mut urls: Vec<String> = Vec::new();
    for url in listed.unwrap_or_default() {
        let url = url.trim().to_owned();
        if !url.is_empty() && !urls.contains(&url) {
            urls.push(url);
        }
    }
    urls
}

/// The `language_servers` of the default settings file: the base layer the user's
/// own `language_servers` is laid over.
fn default_servers_setting() -> &'static Value {
    static SETTING: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    SETTING.get_or_init(|| {
        let defaults: Result<Map<String, Value>, _> =
            serde_json::from_str(&strip_jsonc(DEFAULT_SETTINGS_JSONC));
        defaults
            .ok()
            .and_then(|mut map| map.remove("language_servers"))
            .unwrap_or_else(|| Value::Object(Map::new()))
    })
}

/// The servers there are with the user's file saying nothing: those of the
/// default settings file.
fn default_language_servers() -> &'static LanguageServers {
    static SERVERS: std::sync::OnceLock<LanguageServers> = std::sync::OnceLock::new();
    SERVERS.get_or_init(|| {
        LanguageServers::layered(default_servers_setting(), &Value::Object(Map::new()))
            .unwrap_or_default()
    })
}

/// The names of the settings a settings file has, in file order.
pub(crate) fn keys_of(source: &str) -> Result<Vec<String>, SettingsError> {
    let cleaned = strip_jsonc(source);
    if cleaned.trim().is_empty() {
        return Ok(Vec::new());
    }
    let Pairs(pairs) = serde_json::from_str(&cleaned)?;
    Ok(pairs.into_iter().map(|(key, _)| key).collect())
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

/// A whole-number setting that must lie in `range`.
fn take_in_range(
    map: &mut Map<String, Value>,
    key: &'static str,
    range: std::ops::RangeInclusive<usize>,
) -> Result<Option<usize>, SettingsError> {
    let Some(value) = take::<i64>(map, key)? else {
        return Ok(None);
    };
    match usize::try_from(value) {
        Ok(number) if range.contains(&number) => Ok(Some(number)),
        _ => Err(SettingsError::OutOfRange {
            key,
            value,
            min: *range.start(),
            max: *range.end(),
        }),
    }
}

/// The `sequence_timeout` setting: milliseconds, `0` for no timeout. The outer
/// `None` is a setting that is not there.
fn take_sequence_timeout(
    map: &mut Map<String, Value>,
) -> Result<Option<Option<Duration>>, SettingsError> {
    let Some(value) = take::<i64>(map, "sequence_timeout")? else {
        return Ok(None);
    };
    match usize::try_from(value) {
        Ok(0) => Ok(Some(None)),
        Ok(millis) if SEQUENCE_TIMEOUT_RANGE.contains(&millis) => {
            Ok(Some(Some(Duration::from_millis(millis as u64))))
        }
        _ => Err(SettingsError::OutOfRange {
            key: "sequence_timeout",
            value,
            min: *SEQUENCE_TIMEOUT_RANGE.start(),
            max: *SEQUENCE_TIMEOUT_RANGE.end(),
        }),
    }
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
