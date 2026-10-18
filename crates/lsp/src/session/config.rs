//! Which language servers there are, and which one a file gets: the
//! `language_servers` setting, matching by extension, and finding the root of a
//! server's project.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

use crate::cargo;
use trcode_files::paths::{canonical, without_verbatim_prefix};

/// One language server: what to run, and which files and projects it is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    /// The program to run.
    pub command: String,
    pub args: Vec<String>,
    /// The extensions of the files it serves, without the dot and in lower case.
    pub extensions: Vec<String>,
    /// Files whose presence in a folder makes it the root of a project for this
    /// server (`Cargo.toml`). None means the project root is always the root.
    pub root_files: Vec<String>,
    /// Sent to the server as it is, when initializing. `null` sends none.
    pub initialization_options: Value,
    /// What the server is told a file is, by bare file name (`settings.json`:
    /// `jsonc`), over the extension rule and the built-in names.
    pub language_ids: BTreeMap<String, String>,
    /// Whether saving a file of this server first formats it with the server.
    pub format_on_save: bool,
}

/// What a `language_servers` entry in the settings file may say. A word that is
/// not one of these is refused, so a misspelled one is not silently ignored.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    extensions: Vec<String>,
    #[serde(default)]
    root_files: Vec<String>,
    #[serde(default)]
    initialization_options: Value,
    #[serde(default)]
    language_ids: BTreeMap<String, String>,
    #[serde(default)]
    format_on_save: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConfigError {
    #[error("\"language_servers\" must be an object, {{ ... }}")]
    NotAnObject,
    #[error("\"language_servers\": \"{name}\": {problem}")]
    Entry { name: String, problem: String },
}

/// Which server a file gets, and for which project.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ServerKey {
    /// The name the server has in the settings.
    pub name: String,
    /// The root of the project it serves.
    pub root: PathBuf,
}

/// The servers, by the name they have in the settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageServers {
    servers: BTreeMap<String, ServerConfig>,
}

impl Default for LanguageServers {
    /// No servers: the ones there are come from the settings (the default settings
    /// file, with the user's file laid over it), see [`LanguageServers::layered`].
    fn default() -> Self {
        Self {
            servers: BTreeMap::new(),
        }
    }
}

impl LanguageServers {
    /// The servers `defaults` names with the user's `language_servers` laid over
    /// them, by name and by field: an entry for a server that is in `defaults`
    /// overrides only the fields it has and keeps the others, `null` removes a
    /// server, and an entry with another name adds one (which needs its
    /// `command` and `extensions`). Both are objects, `{ ... }`.
    pub fn layered(defaults: &Value, user: &Value) -> Result<Self, ConfigError> {
        let (Value::Object(base), Value::Object(user)) = (defaults, user) else {
            return Err(ConfigError::NotAnObject);
        };
        let mut merged = base.clone();
        for (name, entry) in user {
            match (merged.get_mut(name), entry) {
                (_, Value::Null) => {
                    merged.remove(name);
                }
                (Some(Value::Object(fields)), Value::Object(over)) => {
                    for (field, value) in over {
                        fields.insert(field.clone(), value.clone());
                    }
                }
                _ => {
                    merged.insert(name.clone(), entry.clone());
                }
            }
        }
        let mut servers = BTreeMap::new();
        for (name, entry) in merged {
            let problem = |problem: String| ConfigError::Entry {
                name: name.clone(),
                problem,
            };
            let entry: Entry = serde_json::from_value(entry).map_err(|e| problem(e.to_string()))?;
            servers.insert(name.clone(), entry.into_config().map_err(problem)?);
        }
        Ok(Self { servers })
    }

    /// The names of the servers, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.servers.keys().map(String::as_str)
    }

    pub fn get(&self, name: &str) -> Option<&ServerConfig> {
        self.servers.get(name)
    }

    /// The server for the file at `path`, going by its extension; the first by
    /// name when several want it.
    pub fn for_file(&self, path: &Path) -> Option<(&str, &ServerConfig)> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        self.servers
            .iter()
            .find(|(_, server)| server.extensions.contains(&extension))
            .map(|(name, server)| (name.as_str(), server))
    }

    /// The server and the project for the file at `path`, when there is one.
    /// See [`root_for`] for the project.
    pub fn resolve(&self, path: &Path, project_root: &Path) -> Option<(ServerKey, &ServerConfig)> {
        let (name, server) = self.for_file(path)?;
        let root = root_for(server, path, project_root)?;
        let key = ServerKey {
            name: name.to_owned(),
            root,
        };
        Some((key, server))
    }
}

impl Entry {
    fn into_config(self) -> Result<ServerConfig, String> {
        let command = self.command.trim().to_owned();
        if command.is_empty() {
            return Err("\"command\" is empty".to_owned());
        }
        let mut extensions = Vec::new();
        for extension in &self.extensions {
            let extension = extension
                .trim()
                .trim_start_matches('.')
                .to_ascii_lowercase();
            if extension.is_empty() || extension.contains(['/', '\\']) {
                return Err(format!("\"{extension}\" is not an extension"));
            }
            extensions.push(extension);
        }
        if extensions.is_empty() {
            return Err("\"extensions\" needs at least one, for example [\"rs\"]".to_owned());
        }
        if let Some(bad) = self
            .root_files
            .iter()
            .find(|name| name.trim().is_empty() || name.contains(['/', '\\']))
        {
            return Err(format!("\"{bad}\" is not a file name for \"root_files\""));
        }
        if let Some((name, id)) = self.language_ids.iter().find(|(name, id)| {
            name.trim().is_empty() || name.contains(['/', '\\']) || id.trim().is_empty()
        }) {
            return Err(format!(
                "\"{name}\": \"{id}\" is not a file name and a language for \"language_ids\""
            ));
        }
        Ok(ServerConfig {
            command,
            args: self.args,
            extensions,
            root_files: self.root_files,
            initialization_options: self.initialization_options,
            language_ids: self.language_ids,
            format_on_save: self.format_on_save,
        })
    }
}

/// The root of the project a server serves for the file at `path`: the
/// outermost folder, from the file's folder up to the project root, that holds
/// one of the server's `root_files`. So a Cargo workspace with many member
/// crates is one project, not many. None found, or no `root_files`, gives the
/// project root.
///
/// For Cargo the outermost folder only counts when its `Cargo.toml` takes the crate
/// of the file in (a workspace member, or a path dependency; see [`cargo::includes`]):
/// a crate that only sits in a folder below it would not be checked at all, so
/// the next folder in from the outside that does take it in is the root, and at
/// the end the crate's own folder.
///
/// A file outside the project root has no project root to fall back on: the
/// nearest folder up the tree that holds a root file is its root, and with
/// none it gets no server (`None`).
pub fn root_for(server: &ServerConfig, path: &Path, project_root: &Path) -> Option<PathBuf> {
    let project_root = without_verbatim_prefix(&canonical(project_root));
    let file = without_verbatim_prefix(&canonical(path));
    let has_root_file = |dir: &Path| server.root_files.iter().any(|name| dir.join(name).exists());

    let folders: Vec<&Path> = file.ancestors().skip(1).collect();
    if file.starts_with(&project_root) {
        // From the inside out.
        let roots: Vec<&Path> = folders
            .into_iter()
            .take_while(|dir| dir.starts_with(&project_root))
            .filter(|dir| has_root_file(dir))
            .collect();
        let Some(&nearest) = roots.first() else {
            return Some(project_root);
        };
        let takes_in = |outer: &Path| {
            let cargo = |dir: &Path| dir.join("Cargo.toml").exists();
            !(cargo(outer) && cargo(nearest)) || cargo::includes(outer, nearest)
        };
        let root = roots
            .iter()
            .rev()
            .find(|outer| takes_in(outer))
            .copied()
            .unwrap_or(nearest);
        return Some(root.to_path_buf());
    }
    folders
        .into_iter()
        .find(|dir| has_root_file(dir))
        .map(Path::to_path_buf)
}
