//! Finding the plugins the user has and judging their manifests: each plugin is
//! a folder in the plugins folder with a `manifest.toml` and the module it names.

use std::fs;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use crate::plugin_api::{Abi, AbiError, Capability, Manifest};

/// The manifest's name in a plugin's folder.
pub(crate) const MANIFEST_FILE: &str = "manifest.toml";

/// The largest module the editor will read, in bytes.
const MAX_MODULE: u64 = 32 * 1024 * 1024;

/// Why a plugin cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum ManifestError {
    #[error("the manifest cannot be read: {0}")]
    Unreadable(String),
    #[error("the manifest is not valid: {0}")]
    Invalid(String),
    #[error("its id `{id}` is not the name of its folder, `{folder}`")]
    IdIsNotTheFolder { id: String, folder: String },
    #[error("`{0}` is not an id: use lower case letters, digits, `-` and `_`")]
    BadId(String),
    #[error("its module `{0}` is not a file inside its folder")]
    BadModulePath(String),
    #[error(transparent)]
    Abi(#[from] AbiError),
    #[error("it needs TerminalCode {required} or later, and this is {have}")]
    NeedsNewerEditor { required: String, have: String },
    #[error("it asks for a capability this version does not have: `{0}`")]
    UnknownCapability(String),
    #[error("it has two actions called `{0}`")]
    DuplicateAction(String),
    #[error("`{0}` is not an action id: no `:` and not empty")]
    BadActionId(String),
    #[error("its module cannot be read: {0}")]
    ModuleUnreadable(String),
}

/// A plugin that is in the plugins folder and whose manifest is good.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Installed {
    pub dir: PathBuf,
    pub manifest: Manifest,
    /// What it needs to be allowed to do, each once, in the order of
    /// [`Capability::ALL`].
    pub capabilities: Vec<Capability>,
}

impl Installed {
    /// The plugin's id.
    pub(crate) fn id(&self) -> &str {
        &self.manifest.id
    }

    /// The module, read from the plugin's folder.
    pub(crate) fn module(&self) -> Result<Vec<u8>, ManifestError> {
        let path = self.dir.join(&self.manifest.wasm);
        let unreadable = |error: std::io::Error| ManifestError::ModuleUnreadable(error.to_string());
        let size = fs::metadata(&path).map_err(unreadable)?.len();
        if size > MAX_MODULE {
            return Err(ManifestError::ModuleUnreadable(format!(
                "{size} bytes, more than the {MAX_MODULE} allowed"
            )));
        }
        fs::read(&path).map_err(unreadable)
    }
}

/// What the editor is, for judging what a manifest requires of it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Host<'a> {
    pub abi: Abi,
    pub version: &'a str,
}

impl Host<'_> {
    /// This editor.
    pub(crate) const THIS: Host<'static> = Host {
        abi: Abi::CURRENT,
        version: env!("CARGO_PKG_VERSION"),
    };
}

/// What was found in one folder of the plugins folder.
pub(crate) type Found = Result<Installed, (String, ManifestError)>;

/// Every folder in `plugins_dir` that has the file of a manifest, good or not,
/// in the order of the folder names. A missing plugins folder has none.
pub(crate) fn scan(plugins_dir: &Path, host: Host<'_>) -> Vec<Found> {
    let Ok(entries) = fs::read_dir(plugins_dir) else {
        return Vec::new();
    };
    let mut folders: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join(MANIFEST_FILE).is_file())
        // A folder whose name starts with a dot is the installer's workshop.
        .filter(|path| {
            !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        })
        .collect();
    folders.sort();
    folders
        .into_iter()
        .map(|dir| {
            let folder = dir
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            read(&dir, host).map_err(|error| (folder, error))
        })
        .collect()
}

/// The plugin in `dir`: its manifest read and judged.
pub(crate) fn read(dir: &Path, host: Host<'_>) -> Result<Installed, ManifestError> {
    let text = fs::read_to_string(dir.join(MANIFEST_FILE))
        .map_err(|error| ManifestError::Unreadable(error.to_string()))?;
    let manifest: Manifest =
        toml::from_str(&text).map_err(|error| ManifestError::Invalid(error.to_string()))?;
    let capabilities = judge(&manifest, dir, host)?;
    Ok(Installed {
        dir: dir.to_path_buf(),
        manifest,
        capabilities,
    })
}

/// Whether the editor can run the plugin the manifest describes; the
/// capabilities it asks for when it can.
fn judge(
    manifest: &Manifest,
    dir: &Path,
    host: Host<'_>,
) -> Result<Vec<Capability>, ManifestError> {
    let folder = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !is_id(&manifest.id) {
        return Err(ManifestError::BadId(manifest.id.clone()));
    }
    if manifest.id != folder {
        return Err(ManifestError::IdIsNotTheFolder {
            id: manifest.id.clone(),
            folder,
        });
    }
    if !inside_folder(&manifest.wasm) {
        return Err(ManifestError::BadModulePath(manifest.wasm.clone()));
    }
    let abi: Abi = manifest.abi.parse()?;
    host.abi.check(abi)?;
    if let Some(required) = &manifest.requires
        && !at_least(host.version, required)?
    {
        return Err(ManifestError::NeedsNewerEditor {
            required: required.clone(),
            have: host.version.to_owned(),
        });
    }
    let mut asked = Vec::new();
    for name in &manifest.capabilities {
        let capability = Capability::parse(name)
            .ok_or_else(|| ManifestError::UnknownCapability(name.clone()))?;
        if !asked.contains(&capability) {
            asked.push(capability);
        }
    }
    asked.sort_by_key(|capability| Capability::ALL.iter().position(|known| known == capability));
    let mut seen: Vec<&str> = Vec::new();
    for action in &manifest.actions {
        if action.id.is_empty() || action.id.contains(':') {
            return Err(ManifestError::BadActionId(action.id.clone()));
        }
        if seen.contains(&action.id.as_str()) {
            return Err(ManifestError::DuplicateAction(action.id.clone()));
        }
        seen.push(&action.id);
    }
    Ok(asked)
}

/// Lower case letters, digits, `-` and `_`, starting with a letter or digit.
pub(super) fn is_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// A relative path with no way out of the folder it is in.
pub(super) fn inside_folder(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

/// Whether version `candidate` is later than `than`; `false` when either is not a
/// version.
pub(crate) fn is_newer(candidate: &str, than: &str) -> bool {
    matches!(at_least(than, candidate), Ok(false))
}

/// Whether version `have` is `required` or later; both are `major.minor.patch`
/// (the later parts may be left out).
pub(super) fn at_least(have: &str, required: &str) -> Result<bool, ManifestError> {
    let parse = |text: &str| -> Result<[u64; 3], ManifestError> {
        let invalid = || ManifestError::Invalid(format!("`{text}` is not a version such as 0.7.0"));
        let mut parts = [0; 3];
        let mut given = 0;
        for (slot, part) in parts.iter_mut().zip(text.split('.')) {
            *slot = part.parse().map_err(|_| invalid())?;
            given += 1;
        }
        if given == 0 || text.split('.').count() > 3 {
            return Err(invalid());
        }
        Ok(parts)
    };
    Ok(parse(have)? >= parse(required)?)
}

#[cfg(test)]
mod tests;
