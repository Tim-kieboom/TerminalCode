//! The index of the official plugins: a file in the editor's repository
//! (`plugins/index.json`) that says which plugins exist, where their manifest and
//! module are, and the SHA-256 the module must have. A snapshot of it is part of each
//! release and the last one fetched is kept in the config folder. See "Plugin
//! manager" in `docs/design.md`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::manifest::{Host, at_least, is_id};
use crate::plugin_api::{Abi, AbiError};

/// The index format this editor reads. A newer major is "update TerminalCode".
pub(crate) const FORMAT: u32 = 1;

/// Where the index is fetched from.
pub(crate) const INDEX_URL: &str =
    "https://raw.githubusercontent.com/Tim-kieboom/TerminalCode/main/plugins/index.json";

/// The index of the release, for when nothing was fetched and nothing is cached.
const SNAPSHOT: &str = include_str!("../../plugins/index.json");

/// The cache's name in the config folder.
const CACHE_FILE: &str = "plugin-index.json";

/// Why an index cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum IndexError {
    #[error("the index is not valid: {0}")]
    Invalid(String),
    #[error("the index is format {0}, which needs a newer TerminalCode")]
    NewerFormat(u32),
    #[error("the entry for `{id}` is not valid: {reason}")]
    BadEntry { id: String, reason: String },
}

/// One official plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub version: String,
    /// Where `manifest.toml` is.
    pub manifest: String,
    /// Where the module is.
    pub module: String,
    /// The SHA-256 of the module, in lower case hex.
    pub sha256: String,
    /// The plugin API it needs.
    pub abi: String,
    /// The oldest TerminalCode it works with.
    #[serde(default)]
    pub requires: Option<String>,
}

/// Whether this editor can run an entry, and what it lacks when it cannot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Availability {
    Ready,
    NeedsEditor(String),
    NeedsApi(AbiError),
}

impl Availability {
    /// What is missing, for the list; empty when nothing is.
    pub(crate) fn missing(&self) -> Option<String> {
        match self {
            Self::Ready => None,
            Self::NeedsEditor(version) => Some(format!("needs TerminalCode {version}")),
            Self::NeedsApi(error) => Some(error.to_string()),
        }
    }
}

impl Entry {
    /// Whether `host` can run this plugin.
    pub(crate) fn availability(&self, host: Host<'_>) -> Availability {
        if let Some(required) = &self.requires
            && !at_least(host.version, required).unwrap_or(false)
        {
            return Availability::NeedsEditor(required.clone());
        }
        match self.abi.parse::<Abi>().map(|abi| host.abi.check(abi)) {
            Ok(Ok(())) => Availability::Ready,
            Ok(Err(error)) | Err(error) => Availability::NeedsApi(error),
        }
    }

    fn check(&self) -> Result<(), IndexError> {
        let bad = |reason: &str| IndexError::BadEntry {
            id: self.id.clone(),
            reason: reason.to_owned(),
        };
        if !is_id(&self.id) {
            return Err(bad("the id is not lower case letters, digits, `-` and `_`"));
        }
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(bad("the sha256 is not 64 hex digits in lower case"));
        }
        if self.abi.parse::<Abi>().is_err() {
            return Err(bad("the abi is not a version such as 1.3"));
        }
        if let Some(required) = &self.requires
            && at_least("0.0.0", required).is_err()
        {
            return Err(bad("`requires` is not a version such as 0.7.0"));
        }
        Ok(())
    }
}

/// The official plugins.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Index {
    pub entries: Vec<Entry>,
}

impl Index {
    pub(crate) fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// The index in `text`.
    pub(crate) fn parse(text: &str) -> Result<Self, IndexError> {
        #[derive(Deserialize)]
        struct Head {
            format: u32,
        }
        #[derive(Deserialize)]
        struct Body {
            #[serde(default)]
            plugins: Vec<Entry>,
        }
        let head: Head =
            serde_json::from_str(text).map_err(|error| IndexError::Invalid(error.to_string()))?;
        if head.format > FORMAT {
            return Err(IndexError::NewerFormat(head.format));
        }
        let body: Body =
            serde_json::from_str(text).map_err(|error| IndexError::Invalid(error.to_string()))?;
        for entry in &body.plugins {
            entry.check()?;
        }
        Ok(Self {
            entries: body.plugins,
        })
    }

    /// The index of this release. Empty in tests, so that what ships does not change
    /// what they see (`the_snapshot_of_the_release_reads` reads the real file).
    pub(crate) fn snapshot() -> Self {
        match cfg!(test) {
            true => Self::default(),
            false => Self::parse(SNAPSHOT).unwrap_or_default(),
        }
    }

    /// The last index that was fetched, or the snapshot when there is none (or it
    /// does not read).
    pub(crate) fn cached_or_snapshot(config_dir: Option<&Path>) -> Self {
        config_dir
            .and_then(|dir| fs::read_to_string(cache_path(dir)).ok())
            .and_then(|text| Self::parse(&text).ok())
            .unwrap_or_else(Self::snapshot)
    }
}

/// Keeps the index text that was fetched, once it is known to be good.
pub(crate) fn store_cache(config_dir: &Path, text: &str) -> std::io::Result<()> {
    let path = cache_path(config_dir);
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, text)?;
    fs::rename(&temp, &path)
}

fn cache_path(config_dir: &Path) -> PathBuf {
    config_dir.join(CACHE_FILE)
}

#[cfg(test)]
mod tests;
