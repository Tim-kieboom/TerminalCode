//! The runnable targets of a cargo project, from `cargo metadata --no-deps`.
//!
//! Cargo finds the binaries and examples of a package by itself (`src/main.rs`,
//! `src/bin/*.rs`, `examples/*.rs`), and a workspace names its packages with globs, so the
//! list is asked of cargo and not read from `Cargo.toml`. Only what can be started is kept:
//! a target of kind `bin` or `example` whose crate type is `bin` (an example that is a
//! library cannot be run). Tests and benches are not runnable programs here.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

use serde::Deserialize;
use thiserror::Error;

/// How much of what cargo said on its standard error is kept to explain a failure.
const MOST_MESSAGE: usize = 600;

/// What a target is. A binary sorts before an example.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Bin,
    Example,
}

impl Kind {
    /// The word cargo uses for it, and the one in a [`TargetId`].
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bin => "bin",
            Self::Example => "example",
        }
    }

    /// The flag that builds it: `--bin` or `--example`.
    pub fn flag(self) -> &'static str {
        match self {
            Self::Bin => "--bin",
            Self::Example => "--example",
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Why `package:kind:name` is not a target.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IdError {
    #[error("`{0}` is not `package:kind:name`")]
    Shape(String),
    #[error("`{0}` is not a kind of target here (`bin` or `example`)")]
    Kind(String),
}

/// Which target: the names of a target alone are not unique in a workspace (two packages
/// can have a binary called `server`, and a binary and an example can share a name).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetId {
    package: String,
    kind: Kind,
    name: String,
}

impl TargetId {
    pub fn new(package: impl Into<String>, kind: Kind, name: impl Into<String>) -> Self {
        Self {
            package: package.into(),
            kind,
            name: name.into(),
        }
    }

    pub fn package(&self) -> &str {
        &self.package
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for TargetId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}:{}", self.package, self.kind, self.name)
    }
}

impl FromStr for TargetId {
    type Err = IdError;

    fn from_str(text: &str) -> Result<Self, IdError> {
        let parts: Vec<&str> = text.split(':').collect();
        let [package, kind, name] = parts.as_slice() else {
            return Err(IdError::Shape(text.to_owned()));
        };
        if package.is_empty() || name.is_empty() {
            return Err(IdError::Shape(text.to_owned()));
        }
        let kind = match *kind {
            "bin" => Kind::Bin,
            "example" => Kind::Example,
            other => return Err(IdError::Kind(other.to_owned())),
        };
        Ok(Self::new(*package, kind, *name))
    }
}

/// One runnable target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    id: TargetId,
    source: PathBuf,
    manifest: PathBuf,
    required_features: Vec<String>,
}

impl Target {
    pub fn id(&self) -> &TargetId {
        &self.id
    }

    /// The file the target starts in (`src/main.rs`).
    pub fn source(&self) -> &Path {
        &self.source
    }

    /// The `Cargo.toml` of its package.
    pub fn manifest(&self) -> &Path {
        &self.manifest
    }

    /// The features cargo will not build the target without.
    pub fn required_features(&self) -> &[String] {
        &self.required_features
    }
}

/// Why the targets are not known.
#[derive(Debug, Error)]
pub enum MetadataError {
    #[error("cargo is not installed, or not on the PATH")]
    CargoMissing,
    #[error("cargo could not be started: {0}")]
    Start(#[source] io::Error),
    #[error("cargo metadata failed: {0}")]
    Failed(String),
    #[error("what cargo metadata printed is not what was expected: {0}")]
    Unreadable(#[source] serde_json::Error),
}

/// The runnable targets of a project, packages in the order cargo gives them, each package's
/// binaries before its examples, by name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Targets {
    targets: Vec<Target>,
}

impl Targets {
    /// Asks cargo, in `root`. This runs a program and may take a while on a big workspace,
    /// so it is not for the thread that draws.
    pub fn fetch(root: &Path) -> Result<Self, MetadataError> {
        Self::fetch_with("cargo", root)
    }

    /// [`Targets::fetch`] with another program in the place of `cargo`.
    pub fn fetch_with(program: &str, root: &Path) -> Result<Self, MetadataError> {
        let output = Command::new(program)
            .args(["metadata", "--no-deps", "--format-version", "1"])
            .current_dir(root)
            .output()
            .map_err(|error| match error.kind() {
                io::ErrorKind::NotFound => MetadataError::CargoMissing,
                _ => MetadataError::Start(error),
            })?;
        if !output.status.success() {
            return Err(MetadataError::Failed(first_words(&output.stderr)));
        }
        Self::from_metadata(&output.stdout)
    }

    /// Reads what `cargo metadata --no-deps --format-version 1` printed.
    pub fn from_metadata(json: &[u8]) -> Result<Self, MetadataError> {
        let metadata: Metadata = serde_json::from_slice(json).map_err(MetadataError::Unreadable)?;
        let mut targets = Vec::new();
        for package in metadata.packages {
            let mut found: Vec<Target> = package
                .targets
                .into_iter()
                .filter_map(|target| runnable(&package.name, &package.manifest_path, target))
                .collect();
            found.sort_by(|a, b| a.id.cmp(&b.id));
            targets.append(&mut found);
        }
        Ok(Self { targets })
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Target> {
        self.targets.iter()
    }

    pub fn len(&self) -> usize {
        self.targets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    pub fn get(&self, id: &TargetId) -> Option<&Target> {
        self.targets.iter().find(|target| target.id == *id)
    }
}

impl<'a> IntoIterator for &'a Targets {
    type Item = &'a Target;
    type IntoIter = std::slice::Iter<'a, Target>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// The target as a runnable one, or `None` when it is not (a library, a test, a bench, an
/// example that is a library).
fn runnable(package: &str, manifest: &Path, target: RawTarget) -> Option<Target> {
    let kind = match target.kind.as_slice() {
        [kind] if kind == "bin" => Kind::Bin,
        [kind] if kind == "example" => Kind::Example,
        _ => return None,
    };
    if !target
        .crate_types
        .iter()
        .any(|crate_type| crate_type == "bin")
    {
        return None;
    }
    Some(Target {
        id: TargetId::new(package, kind, target.name),
        source: target.src_path,
        manifest: manifest.to_path_buf(),
        required_features: target.required_features,
    })
}

/// The start of what a failed command said, to tell the user.
fn first_words(text: &[u8]) -> String {
    let text = String::from_utf8_lossy(text);
    let text = text.trim();
    match text.char_indices().nth(MOST_MESSAGE) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_owned(),
    }
}

#[derive(Deserialize)]
struct Metadata {
    #[serde(default)]
    packages: Vec<RawPackage>,
}

#[derive(Deserialize)]
struct RawPackage {
    name: String,
    manifest_path: PathBuf,
    #[serde(default)]
    targets: Vec<RawTarget>,
}

#[derive(Deserialize)]
struct RawTarget {
    name: String,
    #[serde(default)]
    kind: Vec<String>,
    #[serde(default)]
    crate_types: Vec<String>,
    src_path: PathBuf,
    #[serde(default, rename = "required-features")]
    required_features: Vec<String>,
}
