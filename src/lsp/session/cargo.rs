//! Which crates a Cargo project takes in, for choosing the root a language server
//! is started in. rust-analyzer only checks a crate that its root's `Cargo.toml`
//! lists as a workspace member or reaches through a path dependency; a crate that
//! only sits in a folder below it is not checked at all (no diagnostics, hover or go to
//! definition), so it needs a root of its own.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use toml::{Table, Value};

/// How deep path dependencies are followed, so a cycle or a huge tree ends.
const DEPTH: usize = 8;

/// Whether the crate in the folder `inner` is part of the project whose `Cargo.toml`
/// is in the folder `outer`: it is that crate, a member of its workspace, or reached
/// from either by path dependencies.
pub(in crate::lsp) fn includes(outer: &Path, inner: &Path) -> bool {
    let inner = tidy(inner);
    let mut seen = HashSet::new();
    reaches(&tidy(outer), &inner, &mut seen, DEPTH)
}

fn reaches(folder: &Path, wanted: &Path, seen: &mut HashSet<PathBuf>, depth: usize) -> bool {
    if folder == wanted {
        return true;
    }
    if depth == 0 || !seen.insert(folder.to_path_buf()) {
        return false;
    }
    let Some(manifest) = read(folder) else {
        return false;
    };
    let mut next: Vec<PathBuf> = path_dependencies(&manifest, folder);
    if let Some(workspace) = manifest.get("workspace").and_then(Value::as_table) {
        next.extend(members(workspace, folder));
        if let Some(table) = workspace.get("dependencies").and_then(Value::as_table) {
            next.extend(paths_in(table, folder));
        }
    }
    next.into_iter()
        .any(|folder| reaches(&folder, wanted, seen, depth - 1))
}

fn read(folder: &Path) -> Option<Table> {
    std::fs::read_to_string(folder.join("Cargo.toml"))
        .ok()?
        .parse()
        .ok()
}

/// The folders of the path dependencies of a manifest, in all its dependency tables
/// (also the ones under `target`).
fn path_dependencies(manifest: &Table, folder: &Path) -> Vec<PathBuf> {
    const TABLES: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];
    let mut found = Vec::new();
    let mut collect = |holder: &Table| {
        for name in TABLES {
            if let Some(table) = holder.get(name).and_then(Value::as_table) {
                found.extend(paths_in(table, folder));
            }
        }
    };
    collect(manifest);
    if let Some(targets) = manifest.get("target").and_then(Value::as_table) {
        for target in targets.values().filter_map(Value::as_table) {
            collect(target);
        }
    }
    found
}

fn paths_in(dependencies: &Table, folder: &Path) -> Vec<PathBuf> {
    dependencies
        .values()
        .filter_map(Value::as_table)
        .filter_map(|dependency| dependency.get("path")?.as_str())
        .map(|path| tidy(&folder.join(path)))
        .collect()
}

/// The folders of the workspace members, without the excluded ones.
fn members(workspace: &Table, folder: &Path) -> Vec<PathBuf> {
    let patterns = |key: &str| -> Vec<String> {
        workspace
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect()
    };
    let excluded: Vec<PathBuf> = patterns("exclude")
        .iter()
        .flat_map(|pattern| expand(folder, pattern))
        .collect();
    patterns("members")
        .iter()
        .flat_map(|pattern| expand(folder, pattern))
        .filter(|member| !excluded.contains(member))
        .collect()
}

/// The folders a member pattern names: `crates/*` is every folder in `crates`; `?` and
/// `*` stand for one character and for any characters of one folder name.
fn expand(root: &Path, pattern: &str) -> Vec<PathBuf> {
    let mut folders = vec![root.to_path_buf()];
    for part in pattern
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
    {
        folders = folders
            .into_iter()
            .flat_map(|folder| step(&folder, part))
            .collect();
    }
    folders.into_iter().map(|folder| tidy(&folder)).collect()
}

fn step(folder: &Path, part: &str) -> Vec<PathBuf> {
    if !part.contains(['*', '?']) {
        return vec![folder.join(part)];
    }
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter(|entry| matches(part, &entry.file_name().to_string_lossy()))
        .map(|entry| entry.path())
        .collect()
}

/// `pattern` against `name`, where `?` is one character and `*` is any run of them.
fn matches(pattern: &str, name: &str) -> bool {
    fn go(pattern: &[char], name: &[char]) -> bool {
        match pattern.split_first() {
            None => name.is_empty(),
            Some(('*', rest)) => (0..=name.len()).any(|skip| go(rest, &name[skip..])),
            Some(('?', rest)) => !name.is_empty() && go(rest, &name[1..]),
            Some((c, rest)) => name.first() == Some(c) && go(rest, &name[1..]),
        }
    }
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    go(&pattern, &name)
}

/// The path without `.` and `..` parts, so two spellings of one folder are equal.
fn tidy(path: &Path) -> PathBuf {
    let mut tidy = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                tidy.pop();
            }
            other => tidy.push(other),
        }
    }
    tidy
}

#[cfg(test)]
mod tests;
