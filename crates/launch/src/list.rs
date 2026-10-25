//! The list of run configurations the picker shows: the detected targets, changed by the
//! entries of the file that name them, then the commands of the file, then the entries
//! whose target is not there (any more), which are kept but cannot be run.

use std::collections::{BTreeMap, HashSet};
use std::fmt;

use crate::file::{FileEntry, LaunchFile, Patch, Problem};
use crate::target::{MetadataError, Target, TargetId, Targets};

/// Which configuration; it is what the editor remembers as the active one. It does not
/// change when other configurations come or go.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConfigId(String);

impl ConfigId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn of_target(target: &TargetId) -> Self {
        Self(target.to_string())
    }

    fn of_command(name: &str) -> Self {
        Self(format!("command:{name}"))
    }
}

impl fmt::Display for ConfigId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<&str> for ConfigId {
    fn from(text: &str) -> Self {
        Self(text.to_owned())
    }
}

/// What a configuration starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum What {
    /// A target found by cargo: built, then its executable is started.
    Cargo(Target),
    /// A command of the file: the program, started as it is.
    Command(String),
    /// An entry of the file for a target that is not there (any more).
    Missing(TargetId),
}

/// One thing that can be run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunConfig {
    id: ConfigId,
    name: String,
    what: What,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    cwd: Option<String>,
    release: bool,
    /// Whether the file decides what runs: a command of it, or a target it changes.
    from_file: bool,
}

impl RunConfig {
    /// A program that is already built, to be debugged for something that is not the project's
    /// own list (a plugin's test): it is no entry of the list, and runs with `args` and `env`
    /// in `cwd` (a whole path, or from the project's folder).
    pub fn built_program(
        id: ConfigId,
        name: impl Into<String>,
        program: impl Into<String>,
        args: Vec<String>,
        env: BTreeMap<String, String>,
        cwd: Option<String>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            what: What::Command(program.into()),
            args,
            env,
            cwd,
            release: false,
            from_file: false,
        }
    }

    pub fn id(&self) -> &ConfigId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn what(&self) -> &What {
        &self.what
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn env(&self) -> &BTreeMap<String, String> {
        &self.env
    }

    /// The folder to run in, as the file wrote it (relative to the project, or absolute).
    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    /// Whether the target is built with `--release`.
    pub fn release(&self) -> bool {
        self.release
    }

    /// Whether `.trcode/launch.toml` decides what this runs: it is a command of the file, or
    /// a target the file changes. A project that is cloned can bring such a file, so what
    /// runs because of it is run only in a folder the user trusts.
    pub fn from_file(&self) -> bool {
        self.from_file
    }

    /// The package it is grouped under; commands of the file have none.
    pub fn package(&self) -> Option<&str> {
        match &self.what {
            What::Cargo(target) => Some(target.id().package()),
            What::Missing(id) => Some(id.package()),
            What::Command(_) => None,
        }
    }

    /// Whether it can be run: a target that is not there cannot.
    pub fn is_available(&self) -> bool {
        !matches!(self.what, What::Missing(_))
    }

    /// Why it cannot be run, for the picker to show beside it.
    pub fn unavailable(&self) -> Option<&'static str> {
        match self.what {
            What::Missing(_) => Some("target not found"),
            _ => None,
        }
    }

    fn detected(target: &Target) -> Self {
        let id = target.id();
        Self {
            id: ConfigId::of_target(id),
            name: format!("{} ({})", id.name(), id.kind()),
            what: What::Cargo(target.clone()),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            release: false,
            from_file: false,
        }
    }

    fn missing(target: &TargetId, patch: &Patch) -> Self {
        let mut config = Self {
            id: ConfigId::of_target(target),
            name: format!("{} ({})", target.name(), target.kind()),
            what: What::Missing(target.clone()),
            args: Vec::new(),
            env: BTreeMap::new(),
            cwd: None,
            release: false,
            from_file: true,
        };
        config.apply(patch);
        config
    }

    fn apply(&mut self, patch: &Patch) {
        self.from_file = true;
        if let Some(name) = &patch.name {
            self.name.clone_from(name);
        }
        if let Some(args) = &patch.args {
            self.args.clone_from(args);
        }
        if let Some(env) = &patch.env {
            self.env.clone_from(env);
        }
        if let Some(cwd) = &patch.cwd {
            self.cwd = Some(cwd.clone());
        }
        if let Some(release) = patch.release {
            self.release = release;
        }
    }
}

/// The run configurations, and what to tell the user about how they were made.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunList {
    configs: Vec<RunConfig>,
    problems: Vec<Problem>,
    note: Option<String>,
}

impl RunList {
    /// Puts the targets cargo found (or why it found none) and the file together. Without
    /// targets the list is the file's entries, and the note says why.
    pub fn build(targets: Result<&Targets, &MetadataError>, file: &LaunchFile) -> Self {
        let mut list = Self {
            problems: file.problems().to_vec(),
            ..Self::default()
        };
        let mut detected: Vec<RunConfig> = Vec::new();
        match targets {
            Ok(targets) => detected.extend(targets.iter().map(RunConfig::detected)),
            Err(error) => list.note = Some(error.to_string()),
        }
        let mut commands = Vec::new();
        let mut missing = Vec::new();
        let mut changed: HashSet<TargetId> = HashSet::new();
        let mut named: HashSet<String> = HashSet::new();
        for (number, entry) in file.entries() {
            match entry {
                FileEntry::Override { target, patch } => {
                    if !changed.insert(target.clone()) {
                        let message =
                            format!("`{target}` is changed twice; the first entry counts");
                        list.problems.push(Problem::of_entry(*number, message));
                        continue;
                    }
                    match detected.iter_mut().find(|config| is_target(config, target)) {
                        Some(config) => config.apply(patch),
                        None => missing.push(RunConfig::missing(target, patch)),
                    }
                }
                FileEntry::Command {
                    name,
                    program,
                    args,
                    env,
                    cwd,
                } => {
                    if !named.insert(name.clone()) {
                        let message = format!("there is a command called `{name}` already");
                        list.problems.push(Problem::of_entry(*number, message));
                        continue;
                    }
                    commands.push(RunConfig {
                        id: ConfigId::of_command(name),
                        name: name.clone(),
                        what: What::Command(program.clone()),
                        args: args.clone(),
                        env: env.clone(),
                        cwd: cwd.clone(),
                        release: false,
                        from_file: true,
                    });
                }
            }
        }
        list.configs = detected;
        list.configs.append(&mut commands);
        list.configs.append(&mut missing);
        list
    }

    pub fn iter(&self) -> std::slice::Iter<'_, RunConfig> {
        self.configs.iter()
    }

    pub fn len(&self) -> usize {
        self.configs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.configs.is_empty()
    }

    pub fn get(&self, id: &ConfigId) -> Option<&RunConfig> {
        self.configs.iter().find(|config| config.id == *id)
    }

    /// What is wrong in the file.
    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }

    /// Why there are no detected targets, when cargo could not say.
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }
}

impl<'a> IntoIterator for &'a RunList {
    type Item = &'a RunConfig;
    type IntoIter = std::slice::Iter<'a, RunConfig>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

fn is_target(config: &RunConfig, target: &TargetId) -> bool {
    matches!(&config.what, What::Cargo(found) if found.id() == target)
}
