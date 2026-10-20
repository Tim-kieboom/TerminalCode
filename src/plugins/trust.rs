//! Trust on first use: the first time a version of a plugin is installed, the hash
//! of its module is pinned in `.trust.json` in the plugins folder. From then on that
//! version must come with the same hash. A published version that changes (a release
//! asset replaced, an index edited) is refused instead of installed. Removing a
//! plugin keeps its pins, and a pin is dropped only by editing the file.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The pins' file in the plugins folder.
pub(crate) const TRUST_FILE: &str = ".trust.json";

/// Why a module cannot be trusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TrustError {
    /// The version was installed before with another module.
    Changed {
        id: String,
        version: String,
        pinned: String,
        offered: String,
    },
    /// The pins cannot be read or written; nothing is installed without them.
    File(String),
}

impl std::fmt::Display for TrustError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Changed {
                id,
                version,
                pinned,
                offered,
            } => write!(
                f,
                "{id} {version} has another module than before ({} then, {} now): a published version must not change; if it is meant to, delete its line in {TRUST_FILE}",
                short(pinned),
                short(offered)
            ),
            Self::File(reason) => {
                write!(f, "the plugin pins ({TRUST_FILE}) cannot be used: {reason}")
            }
        }
    }
}

impl std::error::Error for TrustError {}

fn short(hash: &str) -> &str {
    hash.get(..12).unwrap_or(hash)
}

/// The hashes seen so far: plugin id, then version.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Pins {
    #[serde(default)]
    pins: BTreeMap<String, BTreeMap<String, String>>,
}

impl Pins {
    /// The pins in `plugins_dir`: none when there is no file yet, an error when it is
    /// there and does not read (it is never replaced silently).
    pub(crate) fn load(plugins_dir: &Path) -> Result<Self, TrustError> {
        match fs::read_to_string(path(plugins_dir)) {
            Ok(text) => {
                serde_json::from_str(&text).map_err(|error| TrustError::File(error.to_string()))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(TrustError::File(error.to_string())),
        }
    }

    /// Whether `hash` is what `id` `version` had when it was first seen; a version
    /// not seen before is `Ok(true)`: it is new and must be pinned.
    pub(crate) fn check(&self, id: &str, version: &str, hash: &str) -> Result<bool, TrustError> {
        match self.pins.get(id).and_then(|versions| versions.get(version)) {
            None => Ok(true),
            Some(pinned) if pinned == hash => Ok(false),
            Some(pinned) => Err(TrustError::Changed {
                id: id.to_owned(),
                version: version.to_owned(),
                pinned: pinned.clone(),
                offered: hash.to_owned(),
            }),
        }
    }

    /// Checks the pin and, for a new version, writes it.
    pub(crate) fn trust(
        plugins_dir: &Path,
        id: &str,
        version: &str,
        hash: &str,
    ) -> Result<(), TrustError> {
        let mut pins = Self::load(plugins_dir)?;
        if !pins.check(id, version, hash)? {
            return Ok(());
        }
        pins.pins
            .entry(id.to_owned())
            .or_default()
            .insert(version.to_owned(), hash.to_owned());
        pins.save(plugins_dir)
    }

    fn save(&self, plugins_dir: &Path) -> Result<(), TrustError> {
        let file = path(plugins_dir);
        let temp = file.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self)
            .map_err(|error| TrustError::File(error.to_string()))?;
        fs::create_dir_all(plugins_dir)
            .and_then(|()| fs::write(&temp, text))
            .and_then(|()| fs::rename(&temp, &file))
            .map_err(|error| TrustError::File(error.to_string()))
    }
}

/// Checks `module`, read from the plugin folder `dir`, before it runs: when the
/// installer made the folder, the module must be the one it installed (the marker's
/// hash) and the one pinned for its version. A folder the user made is not checked.
pub(crate) fn verify_module(dir: &Path, module: &[u8]) -> Result<(), TrustError> {
    let Some(marker) = super::install::marker(dir) else {
        return Ok(());
    };
    let hash = super::install::hex(&Sha256::digest(module));
    let entry = &marker.entry;
    let moved = |pinned: &str| TrustError::Changed {
        id: entry.id.clone(),
        version: entry.version.clone(),
        pinned: pinned.to_owned(),
        offered: hash.clone(),
    };
    if hash != marker.sha256 {
        return Err(moved(&marker.sha256));
    }
    if let Some(plugins_dir) = dir.parent() {
        Pins::load(plugins_dir)?.check(&entry.id, &entry.version, &hash)?;
    }
    Ok(())
}

fn path(plugins_dir: &Path) -> PathBuf {
    plugins_dir.join(TRUST_FILE)
}

#[cfg(test)]
mod tests;
