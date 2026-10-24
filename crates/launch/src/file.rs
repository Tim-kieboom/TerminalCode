//! The file that overrides the detected run configurations and adds commands of its own:
//! `.trcode/launch.toml` in the project.
//!
//! ```toml
//! [[run]]                      # changes a detected target; only what is named changes
//! target = "trcode:bin:trcode"
//! args = ["--recent"]
//! env = { RUST_LOG = "debug" }
//! cwd = "demo"
//! release = true
//!
//! [[run]]                      # a command of its own (nothing goes through a shell)
//! name = "Dev server"
//! command = "npm"
//! args = ["run", "dev"]
//! ```
//!
//! A problem in one entry is said and the entry is left out; the rest still count.

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::path::Path;

use serde::Deserialize;

use crate::target::TargetId;

/// Where the file is, from the root of the project.
pub const FILE: &str = ".trcode/launch.toml";

/// Something wrong in the file, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The `[[run]]` entry it is about, counted from 1; none for the file as a whole.
    entry: Option<usize>,
    message: String,
}

impl Problem {
    pub(crate) fn of_entry(entry: usize, message: impl Into<String>) -> Self {
        Self {
            entry: Some(entry),
            message: message.into(),
        }
    }

    fn of_file(message: impl Into<String>) -> Self {
        Self {
            entry: None,
            message: message.into(),
        }
    }

    pub fn entry(&self) -> Option<usize> {
        self.entry
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.entry {
            Some(entry) => write!(formatter, "entry {entry}: {}", self.message),
            None => formatter.write_str(&self.message),
        }
    }
}

/// What an entry that names a target changes; what is `None` is left as it was.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Patch {
    pub(crate) name: Option<String>,
    pub(crate) args: Option<Vec<String>>,
    pub(crate) env: Option<BTreeMap<String, String>>,
    pub(crate) cwd: Option<String>,
    pub(crate) release: Option<bool>,
}

/// One entry of the file that is fine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FileEntry {
    /// Changes the detected target.
    Override { target: TargetId, patch: Patch },
    /// A command of its own.
    Command {
        name: String,
        program: String,
        args: Vec<String>,
        env: BTreeMap<String, String>,
        cwd: Option<String>,
    },
}

/// What the file says: the entries that are fine, each with its number in the file, and what
/// is wrong with the others.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchFile {
    entries: Vec<(usize, FileEntry)>,
    problems: Vec<Problem>,
}

impl LaunchFile {
    /// Reads `.trcode/launch.toml` of the project at `root`. A project without the file has
    /// no entries and no problem; a file that cannot be read is a problem.
    pub fn load(root: &Path) -> Self {
        let path = root.join(FILE);
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Self::default(),
            Err(error) => Self {
                entries: Vec::new(),
                problems: vec![Problem::of_file(format!(
                    "{} could not be read: {error}",
                    path.display()
                ))],
            },
        }
    }

    /// Reads the text of the file.
    pub fn parse(text: &str) -> Self {
        let table: toml::Table = match text.parse() {
            Ok(table) => table,
            Err(error) => {
                let problem = Problem::of_file(format!("{FILE} is not valid TOML: {error}"));
                return Self {
                    entries: Vec::new(),
                    problems: vec![problem],
                };
            }
        };
        let mut file = Self::default();
        let Some(run) = table.get("run") else {
            return file;
        };
        let toml::Value::Array(items) = run else {
            file.problems.push(Problem::of_file(
                "`run` must be a list of `[[run]]` tables".to_owned(),
            ));
            return file;
        };
        for (at, item) in items.iter().enumerate() {
            let number = at + 1;
            match read_entry(item) {
                Ok(entry) => file.entries.push((number, entry)),
                Err(message) => file.problems.push(Problem::of_entry(number, message)),
            }
        }
        file
    }

    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    pub(crate) fn entries(&self) -> &[(usize, FileEntry)] {
        &self.entries
    }
}

/// An entry as written, before it is known to make sense.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    target: Option<String>,
    name: Option<String>,
    command: Option<String>,
    args: Option<Vec<String>>,
    env: Option<BTreeMap<String, String>>,
    cwd: Option<String>,
    release: Option<bool>,
}

fn read_entry(item: &toml::Value) -> Result<FileEntry, String> {
    let raw: RawEntry = item
        .clone()
        .try_into()
        .map_err(|error: toml::de::Error| error.message().to_owned())?;
    match (raw.target, raw.command) {
        (Some(_), Some(_)) => Err("an entry has a `target` or a `command`, not both".to_owned()),
        (Some(target), None) => override_of(
            &target,
            Patch {
                name: raw.name,
                args: raw.args,
                env: raw.env,
                cwd: raw.cwd,
                release: raw.release,
            },
        ),
        (None, Some(program)) => {
            command_of(raw.name, program, raw.args, raw.env, raw.cwd, raw.release)
        }
        (None, None) => Err(
            "an entry needs a `target` (to change a detected one) or a `command` (to add one)"
                .to_owned(),
        ),
    }
}

fn override_of(target: &str, patch: Patch) -> Result<FileEntry, String> {
    let target: TargetId = target.parse().map_err(|error| format!("{error}"))?;
    if patch.name.as_deref() == Some("") {
        return Err("`name` is empty".to_owned());
    }
    Ok(FileEntry::Override { target, patch })
}

fn command_of(
    name: Option<String>,
    program: String,
    args: Option<Vec<String>>,
    env: Option<BTreeMap<String, String>>,
    cwd: Option<String>,
    release: Option<bool>,
) -> Result<FileEntry, String> {
    let Some(name) = name.filter(|name| !name.is_empty()) else {
        return Err("a command needs a `name`".to_owned());
    };
    if program.is_empty() {
        return Err("`command` is empty".to_owned());
    }
    if release.is_some() {
        return Err("`release` is for a target, not a command".to_owned());
    }
    Ok(FileEntry::Command {
        name,
        program,
        args: args.unwrap_or_default(),
        env: env.unwrap_or_default(),
        cwd,
    })
}
