//! The index of the official plugins: a file in its own repository
//! (<https://github.com/Tim-kieboom/trcode-plugins>, `index.json`) that says which
//! plugins exist, where their manifest and module are, and the SHA-256 the module
//! must have. The editor has no copy of it: the last one fetched is kept in the
//! config folder. See "Plugin manager" in `docs/design.md`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::manifest::{Host, at_least, is_id, is_newer};
use crate::plugin_api::{Abi, AbiError};

/// The index format this editor reads. A newer major is "update TerminalCode".
pub(crate) const FORMAT: u32 = 1;

/// Where the index is fetched from.
pub(crate) const INDEX_URL: &str =
    "https://raw.githubusercontent.com/Tim-kieboom/trcode-plugins/main/index.json";

/// The cache's name in the config folder.
pub(super) const CACHE_FILE: &str = "plugin-index.json";

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
    /// Where its README is (plain text or Markdown), for "d" in the manager. Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readme: Option<String>,
    /// The URL of the index it was listed in; `None` for the official one. Never read
    /// from an index (a source cannot say it is the official one): it is put there by
    /// the editor, and kept in `installed.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
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

    /// Where it comes from, for the list: `owner/repo` of a GitHub index, else the
    /// host. `None` for the official index.
    pub(crate) fn source_label(&self) -> Option<String> {
        self.source.as_deref().map(source_label)
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

/// A short name for the index at `url`: `owner/repo` for GitHub, else the host.
pub(crate) fn source_label(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let mut parts = rest.split('/');
    let host = parts.next().unwrap_or_default();
    match host {
        "raw.githubusercontent.com" | "github.com" => {
            let owner = parts.next().unwrap_or_default();
            let repo = parts.next().unwrap_or_default();
            match (owner.is_empty(), repo.is_empty()) {
                (false, false) => format!("{owner}/{repo}"),
                _ => host.to_owned(),
            }
        }
        _ => host.to_owned(),
    }
}

/// The entry with the latest version.
fn newest(entries: Vec<&Entry>) -> Option<&Entry> {
    entries.into_iter().reduce(
        |best, entry| match is_newer(&entry.version, &best.version) {
            true => entry,
            false => best,
        },
    )
}

/// The official plugins.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Index {
    pub entries: Vec<Entry>,
}

impl Index {
    /// The entry of the plugin `id` this editor should offer: a plugin may be listed
    /// more than once, once for each version, and the newest version this editor can
    /// run wins; when it can run none, the newest, so that the list can say what it
    /// needs.
    pub(crate) fn entry(&self, id: &str) -> Option<&Entry> {
        self.entry_for(id, Host::THIS)
    }

    pub(crate) fn entry_for(&self, id: &str, host: Host<'_>) -> Option<&Entry> {
        let all: Vec<&Entry> = self.entries.iter().filter(|entry| entry.id == id).collect();
        let ready: Vec<&Entry> = all
            .iter()
            .copied()
            .filter(|entry| entry.availability(host) == Availability::Ready)
            .collect();
        newest(ready).or_else(|| newest(all))
    }

    /// The ids of the plugins listed, each once, in the order they first appear.
    pub(crate) fn ids(&self) -> Vec<&str> {
        let mut ids: Vec<&str> = Vec::new();
        for entry in &self.entries {
            if !ids.contains(&entry.id.as_str()) {
                ids.push(&entry.id);
            }
        }
        ids
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
        let mut entries = body.plugins;
        for entry in &mut entries {
            entry.source = None;
        }
        Ok(Self { entries })
    }

    /// The same index, its entries marked as coming from the index at `url`.
    pub(crate) fn listed_by(mut self, url: &str) -> Self {
        for entry in &mut self.entries {
            entry.source = Some(url.to_owned());
        }
        self
    }

    /// The official index and then the extra ones, in order: a plugin name is
    /// taken by the first index that lists it, so a source cannot replace an official
    /// plugin or an earlier source's. The names that were left out come back too.
    pub(crate) fn merged(official: &Index, extra: &[(String, Index)]) -> (Self, Vec<String>) {
        let mut entries = official.entries.clone();
        let mut left_out: Vec<String> = Vec::new();
        for (url, index) in extra {
            for entry in &index.entries {
                let taken_by_other = entries
                    .iter()
                    .any(|have| have.id == entry.id && have.source.as_ref() != Some(url));
                match taken_by_other {
                    true if !left_out.contains(&entry.id) => left_out.push(entry.id.clone()),
                    true => {}
                    false => entries.push(entry.clone()),
                }
            }
        }
        (Self { entries }, left_out)
    }

    /// The last index that was fetched; empty when there is none (or it does not
    /// read): the editor carries no list of its own.
    pub(crate) fn cached(config_dir: Option<&Path>) -> Self {
        config_dir
            .and_then(|dir| fs::read_to_string(cache_path(dir)).ok())
            .and_then(|text| Self::parse(&text).ok())
            .unwrap_or_default()
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
