//! Fetching the index and installing, updating and removing the plugins of the
//! index. Everything here blocks, so the editor runs it on a thread of its own.
//!
//! A plugin is downloaded in full and checked against the index's SHA-256 before a
//! byte of it is in the plugins folder; it is built in `.staging/` inside the
//! plugins folder and moved into place by renames, so a failure at any point leaves
//! what was there. A version's hash is pinned the first time it is installed
//! (`trust.rs`) and must not change after. Folders made here have an [`MARKER_FILE`]; only those are ever
//! updated or removed: a folder without one is a plugin the user made.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::index::{Availability, Entry, Index, IndexError};
use super::manifest::{self, Host, Installed, MANIFEST_FILE, ManifestError};
use super::trust::Pins;

/// Where the index is fetched from.
pub(crate) use super::index::INDEX_URL;

/// The file in a folder the installer made.
pub(crate) const MARKER_FILE: &str = "installed.json";

/// The file of a plugin that says what it is, shown by `d` in the manager. It is
/// published next to `manifest.toml` and installed beside it; a plugin may leave it out.
pub(crate) const DESCRIPTION_FILE: &str = "description.md";

/// The workshop inside the plugins folder.
pub(super) const STAGING: &str = ".staging";

/// How long one download may take, start to end.
const TIMEOUT: Duration = Duration::from_secs(30);
/// The largest manifest or index, in bytes.
const MAX_TEXT: u64 = 1024 * 1024;
/// The largest module, in bytes.
const MAX_MODULE: u64 = 32 * 1024 * 1024;

/// Where downloads may come from.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Policy {
    https_only: bool,
    hosts: &'static [&'static str],
}

impl Policy {
    /// GitHub and the host its release assets are served from.
    pub(crate) const GITHUB: Policy = Policy {
        https_only: true,
        hosts: &[
            "github.com",
            "raw.githubusercontent.com",
            "objects.githubusercontent.com",
            // Where a release asset is sent to today (`objects` was before).
            "release-assets.githubusercontent.com",
        ],
    };

    #[cfg(test)]
    pub(crate) const fn local(hosts: &'static [&'static str]) -> Policy {
        Policy {
            https_only: false,
            hosts,
        }
    }

    pub(super) fn allows(&self, url: &str) -> bool {
        let Some((scheme, rest)) = url.split_once("://") else {
            return false;
        };
        if self.https_only && scheme != "https" || !matches!(scheme, "http" | "https") {
            return false;
        }
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        // A name with a user or a port in front of it is not the host it looks like.
        let host = match authority.rsplit_once(':') {
            Some((host, port)) if !self.https_only && port.bytes().all(|b| b.is_ascii_digit()) => {
                host
            }
            _ => authority,
        };
        self.hosts.contains(&host)
    }
}

/// Why something was not fetched or installed.
#[derive(Debug, Error)]
pub(crate) enum InstallError {
    #[error("stopped")]
    Cancelled,
    #[error("`{0}` is not a place plugins are fetched from")]
    ForbiddenUrl(String),
    #[error("no answer: {0}")]
    Network(String),
    #[error("the server answered {status} for {url}")]
    Status { status: u16, url: String },
    #[error("{url} is more than the {limit} bytes allowed")]
    TooLarge { url: String, limit: u64 },
    #[error("the module does not match what the index promised")]
    HashMismatch,
    #[error("{0}")]
    Untrusted(String),
    #[error("it needs more than this TerminalCode has: {0}")]
    Unavailable(String),
    #[error(
        "a plugin of your own is in {0}: remove it by hand first, the manager only touches what it installed"
    )]
    Local(PathBuf),
    #[error("{0} was not installed by the manager, so it is left alone")]
    NotManaged(String),
    #[error("the plugin that came is not the one in the index: {0}")]
    NotWhatTheIndexSaid(String),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Index(#[from] IndexError),
    #[error("the plugins folder cannot be written: {0}")]
    Io(String),
}

impl From<std::io::Error> for InstallError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

/// What the installer says it is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    Manifest,
    Module,
    Checking,
    Installing,
}

/// What a job on its thread tells the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum JobEvent {
    /// The index was fetched, or why not.
    Index(Result<Index, String>),
    /// The README at `url` was fetched, or why not.
    Readme {
        url: String,
        result: Result<String, String>,
    },
    /// The index of an extra source (`plugin_sources`) was fetched, or why not.
    SourceIndex {
        source: String,
        result: Result<Index, String>,
    },
    /// An install got to a stage.
    Stage { id: Box<str>, stage: Stage },
    /// An install is over: it worked, or why not (`stopped` when the user stopped it).
    Done {
        id: Box<str>,
        update: bool,
        result: Result<(), String>,
    },
}

/// What `installed.json` holds: the entry the folder came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Marker {
    pub entry: Entry,
    /// The hash of the module as it was put in the folder.
    pub sha256: String,
}

/// The marker of the plugin folder `dir`, when the installer made it.
pub(crate) fn marker(dir: &Path) -> Option<Marker> {
    serde_json::from_str(&fs::read_to_string(dir.join(MARKER_FILE)).ok()?).ok()
}

/// Fetches `url`, following redirects only to places `policy` allows, and stopping
/// when `cancel` is set or the body passes `limit` bytes. The download runs on a
/// thread of its own so that a stop is heard at once, even while the server says
/// nothing; that thread ends by itself (at the latest at the time limit) and writes
/// nowhere.
pub(crate) fn fetch(
    url: &str,
    policy: Policy,
    limit: u64,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, InstallError> {
    stop_if(cancel)?;
    let (done, answer) = std::sync::mpsc::channel();
    let url = url.to_owned();
    std::thread::spawn(move || {
        let _ = done.send(fetch_blocking(&url, policy, limit));
    });
    loop {
        match answer.recv_timeout(Duration::from_millis(50)) {
            Ok(result) => return result,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => stop_if(cancel)?,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(InstallError::Network(
                    "the download ended without an answer".to_owned(),
                ));
            }
        }
    }
}

fn fetch_blocking(url: &str, policy: Policy, limit: u64) -> Result<Vec<u8>, InstallError> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    let mut url = url.to_owned();
    for _ in 0..5 {
        if !policy.allows(&url) {
            return Err(InstallError::ForbiddenUrl(url));
        }
        let response = agent
            .get(&url)
            .call()
            .map_err(|error| InstallError::Network(error.to_string()))?;
        let status = response.status().as_u16();
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            let next = response
                .headers()
                .get("location")
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| InstallError::Network("a redirect with no place".to_owned()))?;
            url = next.to_owned();
            continue;
        }
        if status != 200 {
            return Err(InstallError::Status { status, url });
        }
        return read_body(response.into_body().into_reader(), &url, limit);
    }
    Err(InstallError::Network("too many redirects".to_owned()))
}

fn read_body(mut reader: impl Read, url: &str, limit: u64) -> Result<Vec<u8>, InstallError> {
    let mut body = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        let read = reader
            .read(&mut chunk)
            .map_err(|error| InstallError::Network(error.to_string()))?;
        if read == 0 {
            return Ok(body);
        }
        body.extend_from_slice(&chunk[..read]);
        if body.len() as u64 > limit {
            return Err(InstallError::TooLarge {
                url: url.to_owned(),
                limit,
            });
        }
    }
}

fn stop_if(cancel: &AtomicBool) -> Result<(), InstallError> {
    match cancel.load(Ordering::Relaxed) {
        true => Err(InstallError::Cancelled),
        false => Ok(()),
    }
}

/// The address of `file` in the folder of the file at `url`.
pub(crate) fn sibling(url: &str, file: &str) -> String {
    match url.rsplit_once('/') {
        Some((folder, _)) => format!("{folder}/{file}"),
        None => file.to_owned(),
    }
}

/// Fetches a README: text, at most [`MAX_TEXT`] bytes.
pub(crate) fn fetch_readme(
    url: &str,
    policy: Policy,
    cancel: &AtomicBool,
) -> Result<String, InstallError> {
    let body = fetch(url, policy, MAX_TEXT, cancel)?;
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// Fetches the index, and keeps it in the config folder when it is good.
pub(crate) fn fetch_index(
    url: &str,
    policy: Policy,
    config_dir: Option<&Path>,
    cancel: &AtomicBool,
) -> Result<Index, InstallError> {
    let text = String::from_utf8(fetch(url, policy, MAX_TEXT, cancel)?)
        .map_err(|_| IndexError::Invalid("not text".to_owned()))?;
    let index = Index::parse(&text)?;
    if let Some(dir) = config_dir {
        // A cache that cannot be written only means the next start uses the snapshot.
        let _ = super::index::store_cache(dir, &text);
    }
    Ok(index)
}

/// Installs `entry` into `plugins_dir`, or updates the copy the manager made
/// before. Nothing is put in the folder unless the module is the one the index
/// names.
pub(crate) fn install(
    entry: &Entry,
    plugins_dir: &Path,
    host: Host<'_>,
    policy: Policy,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Stage),
) -> Result<Installed, InstallError> {
    if let Availability::NeedsEditor(_) | Availability::NeedsApi(_) = entry.availability(host) {
        let missing = entry.availability(host).missing().unwrap_or_default();
        return Err(InstallError::Unavailable(missing));
    }
    let target = plugins_dir.join(&entry.id);
    if target.exists() && marker(&target).is_none() {
        return Err(InstallError::Local(target));
    }

    progress(Stage::Manifest);
    let manifest_text = fetch(&entry.manifest, policy, MAX_TEXT, cancel)?;
    progress(Stage::Module);
    let module = fetch(&entry.module, policy, MAX_MODULE, cancel)?;

    // The description is not what runs, so it is not checked and it is not needed:
    // a plugin without one, or one that cannot be fetched, installs all the same.
    let description = match fetch(
        &sibling(&entry.manifest, DESCRIPTION_FILE),
        policy,
        MAX_TEXT,
        cancel,
    ) {
        Err(InstallError::Cancelled) => return Err(InstallError::Cancelled),
        other => other.ok(),
    };

    progress(Stage::Checking);
    let hash = hex(&Sha256::digest(&module));
    if hash != entry.sha256 {
        return Err(InstallError::HashMismatch);
    }
    stop_if(cancel)?;

    // Trust on first use: this version's hash is pinned now, or must match its pin.
    Pins::trust(plugins_dir, &entry.id, &entry.version, &hash)
        .map_err(|error| InstallError::Untrusted(error.to_string()))?;

    progress(Stage::Installing);
    let workshop = plugins_dir.join(STAGING);
    let staged = workshop.join(&entry.id);
    let files = Files {
        manifest: &manifest_text,
        module: &module,
        description: description.as_deref(),
        hash: &hash,
    };
    let result = build_and_swap(entry, &files, &staged, &target, host);
    // Whatever is left in the workshop (a failed build, the old copy) goes.
    let _ = fs::remove_dir_all(&workshop);
    result
}

/// What an install puts in the plugin's folder.
struct Files<'a> {
    manifest: &'a [u8],
    module: &'a [u8],
    description: Option<&'a [u8]>,
    /// The hash of the module.
    hash: &'a str,
}

fn build_and_swap(
    entry: &Entry,
    files: &Files<'_>,
    staged: &Path,
    target: &Path,
    host: Host<'_>,
) -> Result<Installed, InstallError> {
    let Files {
        manifest: manifest_text,
        module,
        description,
        hash,
    } = *files;
    let _ = fs::remove_dir_all(staged);
    fs::create_dir_all(staged)?;
    fs::write(staged.join(MANIFEST_FILE), manifest_text)?;
    let described = {
        let text = std::str::from_utf8(manifest_text)
            .map_err(|error| ManifestError::Invalid(error.to_string()))?;
        toml::from_str::<crate::plugin_api::Manifest>(text)
            .map_err(|error| ManifestError::Invalid(error.to_string()))?
    };
    // `read` also refuses a module path that leaves the folder, so check it first.
    let module_file = described.module_file().to_owned();
    if !super::manifest::inside_folder(&module_file) {
        return Err(ManifestError::BadModulePath(module_file).into());
    }
    fs::write(staged.join(&module_file), module)?;
    if let Some(description) = description {
        fs::write(staged.join(DESCRIPTION_FILE), description)?;
    }
    let marker = Marker {
        entry: entry.clone(),
        sha256: hash.to_owned(),
    };
    fs::write(
        staged.join(MARKER_FILE),
        serde_json::to_string_pretty(&marker)
            .map_err(|error| InstallError::Io(error.to_string()))?,
    )?;

    let judged = manifest::read(staged, host)?;
    if judged.manifest.id != entry.id || judged.manifest.version != entry.version {
        return Err(InstallError::NotWhatTheIndexSaid(format!(
            "the manifest is {} {}, the index {} {}",
            judged.manifest.id, judged.manifest.version, entry.id, entry.version
        )));
    }

    let old = staged.with_file_name(format!("{}.old", entry.id));
    let had_old = target.exists();
    if had_old {
        fs::rename(target, &old)?;
    }
    if let Err(error) = fs::rename(staged, target) {
        if had_old {
            let _ = fs::rename(&old, target);
        }
        return Err(error.into());
    }
    Ok(manifest::read(target, host)?)
}

/// Removes the plugin `id` the manager installed.
pub(crate) fn remove(plugins_dir: &Path, id: &str) -> Result<(), InstallError> {
    let dir = plugins_dir.join(id);
    if !super::manifest::is_id(id) || marker(&dir).is_none() {
        return Err(InstallError::NotManaged(id.to_owned()));
    }
    fs::remove_dir_all(&dir)?;
    Ok(())
}

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
