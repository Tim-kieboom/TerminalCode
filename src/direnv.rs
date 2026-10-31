//! The environment a project's `.envrc` sets up, read through `direnv`, so that what the
//! editor starts (a test run, the debugger, a shell) sees the same tools as a shell
//! opened in the project folder. On Nix that is the dev shell of `use flake`.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A variable `direnv` sets, or removes (`None`).
pub(crate) type Change = (String, Option<String>);

/// The nearest `.envrc` in `project` or the folders above it, as `direnv` looks.
pub(crate) fn find_envrc(project: &Path) -> Option<PathBuf> {
    project
        .ancestors()
        .map(|folder| folder.join(".envrc"))
        .find(|file| file.is_file())
}

/// What `direnv` makes of `envrc` for a shell in `project`: the changes to the
/// environment, or what to tell the user when there are none to be had.
pub(crate) fn load(envrc: &Path, project: &Path) -> Result<Vec<Change>, String> {
    load_with(OsStr::new("direnv"), envrc, project)
}

fn load_with(program: &OsStr, envrc: &Path, project: &Path) -> Result<Vec<Change>, String> {
    let output = Command::new(program)
        .args(["export", "json"])
        .current_dir(project)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| match error.kind() {
            ErrorKind::NotFound => {
                format!("{} needs direnv, which is not installed", envrc.display())
            }
            _ => format!("could not run direnv: {error}"),
        })?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!("direnv failed: {}", last_line(&stderr)));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        // Nothing to change: either the file is already loaded or direnv refused it.
        return match stderr.contains("blocked") {
            true => Err(format!(
                "{} is blocked: run `direnv allow` in the folder and start again",
                envrc.display()
            )),
            false => Ok(Vec::new()),
        };
    }
    parse(&stdout)
}

fn last_line(text: &str) -> &str {
    text.lines()
        .rfind(|line| !line.trim().is_empty())
        .unwrap_or("no message")
}

/// Reads the JSON `direnv export json` prints: a map from a variable to its new value,
/// or to `null` when it is to be removed.
pub(crate) fn parse(json: &str) -> Result<Vec<Change>, String> {
    let all: BTreeMap<String, Option<String>> = serde_json::from_str(json)
        .map_err(|error| format!("direnv's output is no JSON: {error}"))?;
    Ok(all
        .into_iter()
        .filter(|change| can_be_set(change))
        .collect())
}

/// Whether the environment can take `change` (setting a name that is empty or has a
/// `=` or a NUL in it, or a value with a NUL, makes the process panic).
fn can_be_set((name, value): &Change) -> bool {
    let plain = |text: &str| !text.contains('\0');
    !name.is_empty() && !name.contains('=') && plain(name) && value.as_deref().is_none_or(plain)
}

/// Puts `changes` in the environment of this process, and so in that of everything it
/// starts after this.
///
/// Call it before the editor starts a thread, when nothing else can be reading the
/// environment.
pub(crate) fn apply(changes: &[Change]) {
    for (name, value) in changes {
        // SAFETY: the caller runs this before any thread is started, so no other
        // thread reads or writes the environment while it changes; `parse` kept only
        // names and values the environment accepts.
        unsafe {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

#[cfg(test)]
mod tests;
