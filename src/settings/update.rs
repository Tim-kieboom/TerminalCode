//! Adding the settings a new version introduced to the user's `settings.json`.
//!
//! The one exception to "the editor never writes `settings.json`": it only ever
//! adds entries that are missing, with the default value and the comments the
//! default file has for it, and never changes or removes what is there. Which
//! settings are new is told by a table of the version that introduced each, and
//! a version kept in `state.json`: only settings introduced after it are added,
//! so one the user deleted stays deleted.
//!
//! Anything that makes the insertion unsure (a file that does not parse, an end
//! that is not one `}` on its own line, a comment after the last entry, unsaved
//! edits in a tab, a file that cannot be written) writes nothing and says why;
//! the version is not advanced, so the next start or reload tries again.

use std::path::Path;

use super::{DEFAULT_SETTINGS_JSONC, Settings, keys_of, strip_jsonc};
use crate::buffer::StagedWrite;
use crate::config::ConfigWarning;
use crate::persist::saved_state::SavedState;

/// A version, as `major.minor.patch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Version(u32, u32, u32);

impl Version {
    /// `0.6.1`, or `0.6.1-dev`; `None` for anything else.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let core = text.trim().split(['-', '+']).next()?;
        let mut parts = core.split('.').map(|part| part.parse::<u32>().ok());
        let version = Self(parts.next()??, parts.next()??, parts.next()??);
        parts.next().is_none().then_some(version)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// What a file that has no stored version counts as: the release before this
/// feature, so only settings introduced after it are added to it.
const BASELINE: &str = "0.6.1";

/// A setting, and the version that introduced it.
struct Introduced {
    key: &'static str,
    /// The version, or [`BASELINE`] for a setting that is older than the table.
    since: &'static str,
    /// The text to add for a setting whose default spans several lines or
    /// otherwise cannot be taken from the default file; `None` takes it from there.
    text: Option<&'static str>,
}

const fn setting(key: &'static str, since: &'static str) -> Introduced {
    Introduced {
        key,
        since,
        text: None,
    }
}

/// Every setting there is. A setting that is added to `defaults/settings.jsonc`
/// has to be added here too, with the version that ships it: a test fails when
/// one is missing, since it would never reach the files that exist.
const INTRODUCED: &[Introduced] = &[
    setting("theme", BASELINE),
    setting("shell", BASELINE),
    setting("mouse", BASELINE),
    setting("tab_size", BASELINE),
    setting("indent_style", BASELINE),
    setting("detect_indent", BASELINE),
    setting("sequence_timeout", BASELINE),
    setting("notification_seconds", BASELINE),
    setting("hide_ignored", BASELINE),
    setting("language_servers", BASELINE),
    setting("format_on_save", "0.7.0"),
];

/// Why the file was not changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The file does not parse, so the error is not buried under an insertion.
    DoesNotParse,
    NotOneObject,
    /// A comment follows the last entry, and the new ones would go between.
    CommentAfterLastEntry,
    /// A tab has the file open with changes that are not saved.
    UnsavedTab,
    /// The default file has no block to take a new setting's text from.
    NoDefaultText(&'static str),
    /// What was made did not read back as settings (a bug here, not the file).
    DoesNotReadBack,
    /// The file could not be read or written.
    Io(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DoesNotParse => f.write_str("the file does not parse"),
            Self::NotOneObject => f.write_str("it is not one { ... }"),
            Self::CommentAfterLastEntry => f.write_str("a comment follows its last entry"),
            Self::UnsavedTab => f.write_str("it is open in a tab with unsaved changes"),
            Self::NoDefaultText(key) => write!(f, "no text for \"{key}\" in the default file"),
            Self::DoesNotReadBack => f.write_str("the result did not read back"),
            Self::Io(error) => f.write_str(error),
        }
    }
}

/// What looking at the file came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Nothing to add and nothing to remember.
    Nothing,
    /// Nothing to add; this version is to be remembered.
    Remember(Version),
    /// Settings were added to the file, and this version is to be remembered.
    Added {
        keys: Vec<&'static str>,
        version: Version,
    },
    Refused(Refusal),
}

/// The newest version of the table: what a file that has everything is up to date with.
fn newest() -> Version {
    INTRODUCED
        .iter()
        .filter_map(|setting| Version::parse(setting.since))
        .max()
        .unwrap_or(Version(0, 0, 0))
}

/// Adds what is new to the file at `path`, as far as `stored` says it has not
/// seen. `unsaved_tab` is whether a tab has the file open with unsaved changes.
pub(crate) fn add_new_settings(path: &Path, stored: Option<Version>, unsaved_tab: bool) -> Outcome {
    let known = stored.unwrap_or_else(|| Version::parse(BASELINE).unwrap_or(Version(0, 0, 0)));
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        // No file: a new one starts from the whole default, and has everything. The
        // version is remembered once the file exists.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Outcome::Nothing,
        Err(error) => {
            return Outcome::Refused(Refusal::Io(format!("it cannot be read: {error}")));
        }
    };
    let (text, keys) = match plan(&source, DEFAULT_SETTINGS_JSONC, known) {
        Ok(Plan::Current) => return remember(stored, newest().max(known)),
        Ok(Plan::Insert { text, keys }) => (text, keys),
        Err(refusal) => return Outcome::Refused(refusal),
    };
    if unsaved_tab {
        return Outcome::Refused(Refusal::UnsavedTab);
    }
    let written = StagedWrite::stage(path, &text).and_then(StagedWrite::commit);
    if let Err(error) = written {
        return Outcome::Refused(Refusal::Io(format!("it cannot be written: {error}")));
    }
    Outcome::Added {
        keys,
        version: newest().max(known),
    }
}

/// What a look at the file came to, for the app to tell and to save.
#[derive(Debug, Default)]
pub(crate) struct Report {
    /// What to tell the user, when settings were added.
    pub(crate) info: Option<String>,
    /// Why the file was not updated, when it was not.
    pub(crate) warning: Option<ConfigWarning>,
    /// Whether the remembered state changed and is to be written.
    pub(crate) remember: bool,
}

/// Looks at the settings file at `path` and adds what is new to it, keeping the
/// version in `saved` up to date. The caller writes `saved` when the report says so.
pub(crate) fn update_file(path: &Path, saved: &mut SavedState, unsaved_tab: bool) -> Report {
    let stored = saved.settings_version().and_then(Version::parse);
    let mut report = Report::default();
    match add_new_settings(path, stored, unsaved_tab) {
        Outcome::Nothing => {}
        Outcome::Remember(version) => {
            report.remember = saved.set_settings_version(&version.to_string());
        }
        Outcome::Added { keys, version } => {
            report.remember = saved.set_settings_version(&version.to_string());
            report.info = Some(format!(
                "added new settings to settings.json: {}",
                keys.join(", ")
            ));
        }
        Outcome::Refused(refusal) => {
            report.warning = Some(ConfigWarning::SettingsNotUpdated {
                reason: refusal.to_string(),
            });
        }
    }
    report
}

/// The version to remember, when it is news.
fn remember(stored: Option<Version>, version: Version) -> Outcome {
    match stored {
        Some(stored) if stored >= version => Outcome::Nothing,
        _ => Outcome::Remember(version),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Plan {
    /// Nothing is missing that this file has not seen.
    Current,
    /// The text of the file with the new settings in it.
    Insert {
        text: String,
        keys: Vec<&'static str>,
    },
}

/// What the file `source` needs added, given that it has seen the settings up
/// to `known`, taking the text of each from `defaults`.
pub(super) fn plan(source: &str, defaults: &str, known: Version) -> Result<Plan, Refusal> {
    Settings::parse(source).map_err(|_| Refusal::DoesNotParse)?;
    let present = keys_of(source).map_err(|_| Refusal::DoesNotParse)?;
    let missing: Vec<&Introduced> = INTRODUCED
        .iter()
        .filter(|setting| Version::parse(setting.since).is_some_and(|since| since > known))
        .filter(|setting| !present.iter().any(|key| key == setting.key))
        .collect();
    if missing.is_empty() {
        return Ok(Plan::Current);
    }

    let mut blocks = Vec::with_capacity(missing.len());
    for setting in &missing {
        let block = match setting.text {
            Some(text) => Some(text.to_owned()),
            None => default_block(defaults, setting.key),
        };
        blocks.push(block.ok_or(Refusal::NoDefaultText(setting.key))?);
    }
    let keys: Vec<&'static str> = missing.iter().map(|setting| setting.key).collect();
    let text = insert(source, &blocks)?;

    // What was made has to read back, with every new setting in it.
    Settings::parse(&text).map_err(|_| Refusal::DoesNotReadBack)?;
    let after = keys_of(&text).map_err(|_| Refusal::DoesNotReadBack)?;
    if !keys.iter().all(|key| after.iter().any(|name| name == key)) {
        return Err(Refusal::DoesNotReadBack);
    }
    Ok(Plan::Insert { text, keys })
}

/// The comments above `key` in the default file and its `"key": value,` line,
/// as they are there, each line with its break. `None` when the entry is not on
/// one line ending in a comma.
pub(super) fn default_block(defaults: &str, key: &str) -> Option<String> {
    let lines: Vec<&str> = defaults.lines().collect();
    let wanted = format!("\"{key}\":");
    let at = lines.iter().position(|line| {
        // Four spaces: a member of the one object, not a line inside a value.
        line.strip_prefix("    ")
            .is_some_and(|rest| rest.starts_with(&wanted))
    })?;
    if !lines[at].trim_end().ends_with(',') {
        return None;
    }
    let mut first = at;
    while first > 0 && lines[first - 1].trim_start().starts_with("//") {
        first -= 1;
    }
    let mut block = String::new();
    for line in &lines[first..=at] {
        block.push_str(line);
        block.push('\n');
    }
    Some(block)
}

/// `source` with `blocks` (each ending in a line break) added after its last entry.
fn insert(source: &str, blocks: &[String]) -> Result<String, Refusal> {
    let chars: Vec<char> = source.chars().collect();
    // Comments and the commas before a closing bracket are spaces here, so the
    // positions are those of the file.
    let plain: Vec<char> = strip_jsonc(source).chars().collect();
    let significant = |index: usize| !plain[index].is_whitespace();
    let open = (0..plain.len())
        .find(|&index| significant(index))
        .filter(|&index| plain[index] == '{')
        .ok_or(Refusal::NotOneObject)?;
    let close = (0..plain.len())
        .rev()
        .find(|&index| significant(index))
        .filter(|&index| plain[index] == '}' && index > open)
        .ok_or(Refusal::NotOneObject)?;
    let last = (open..close)
        .rev()
        .find(|&index| significant(index))
        .ok_or(Refusal::NotOneObject)?;
    let between: String = chars[last + 1..close].iter().collect();

    let mut text = String::with_capacity(source.len() + 256);
    if last == open {
        // An empty object: the entries go inside, on lines of their own.
        text.extend(&chars[..=open]);
        text.push('\n');
        for block in blocks {
            text.push_str(block);
        }
        text.extend(&chars[close..]);
        return Ok(text);
    }
    if between.contains('/') {
        return Err(Refusal::CommentAfterLastEntry);
    }
    let line_start = chars[..close]
        .iter()
        .rposition(|&c| c == '\n')
        .map_or(0, |at| at + 1);
    let own_line = chars[line_start..close].iter().all(|c| c.is_whitespace());
    let has_comma = between.contains(',');
    text.extend(&chars[..=last]);
    if !has_comma || !own_line {
        text.push(',');
    }
    if own_line {
        text.extend(&chars[last + 1..line_start]);
    } else {
        // The `}` shares its line with the last entry: it goes to a line of its own.
        text.push('\n');
    }
    for block in blocks {
        text.push('\n');
        text.push_str(block);
    }
    text.extend(&chars[if own_line { line_start } else { close }..]);
    Ok(text)
}

#[cfg(test)]
mod tests;
