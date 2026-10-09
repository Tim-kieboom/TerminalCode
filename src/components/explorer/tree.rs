//! The directory tree behind the explorer: nodes that load their children on
//! demand, and the flat list of rows the explorer shows.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use thiserror::Error;

use crate::paths::canonical;

/// The version control directory is never listed.
const HIDDEN_DIRECTORY: &str = ".git";

#[derive(Debug, Error)]
pub enum ExplorerError {
    #[error("cannot read {}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: ignore::Error,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeKind {
    Dir,
    File,
}

#[derive(Debug)]
pub(super) struct Node {
    name: String,
    path: PathBuf,
    kind: NodeKind,
    expanded: bool,
    /// `None` until the directory is first opened.
    children: Option<Vec<Node>>,
}

/// Which open directories [`Node::reload`] reads again.
#[derive(Debug, Clone, Copy)]
pub(super) enum Scope<'a> {
    Everything,
    /// Only these directories (as paths the file watcher reports, which are
    /// resolved ones; a node's own spelling is tried too).
    Only(&'a HashSet<PathBuf>),
}

impl Scope<'_> {
    fn includes(&self, dir: &Path) -> bool {
        match self {
            Self::Everything => true,
            Self::Only(dirs) => dirs.contains(dir) || dirs.contains(&canonical(dir)),
        }
    }
}

/// One line of the explorer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    pub(crate) depth: usize,
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) kind: NodeKind,
    pub(crate) expanded: bool,
}

impl Node {
    /// The root of the tree: a directory that starts expanded.
    pub(super) fn root(path: &Path) -> Result<Self, ExplorerError> {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let mut root = Self {
            name,
            path: path.to_path_buf(),
            kind: NodeKind::Dir,
            expanded: false,
            children: None,
        };
        root.expand()?;
        Ok(root)
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    /// Appends this directory and every open directory below it.
    pub(super) fn collect_open_dirs(&self, out: &mut Vec<PathBuf>) {
        if self.kind != NodeKind::Dir || !self.expanded {
            return;
        }
        out.push(self.path.clone());
        for child in self.children.iter().flatten() {
            child.collect_open_dirs(out);
        }
    }

    fn entry(path: PathBuf) -> Self {
        let kind = match path.is_dir() {
            true => NodeKind::Dir,
            false => NodeKind::File,
        };
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            name,
            path,
            kind,
            expanded: false,
            children: None,
        }
    }

    /// Opens a directory, reading it again every time: a closed directory is not
    /// watched, so what was listed when it was closed may be out of date. What
    /// was open inside it stays open. Nothing changes if the read fails.
    fn expand(&mut self) -> Result<(), ExplorerError> {
        if self.kind != NodeKind::Dir {
            return Ok(());
        }
        let fresh = read_children(&self.path)?;
        self.children = Some(self.merge(fresh));
        self.expanded = true;
        Ok(())
    }

    /// Finds the node at `path` among the open directories.
    pub(super) fn find_mut(&mut self, path: &Path) -> Option<&mut Node> {
        if self.path == path {
            return Some(self);
        }
        if !path.starts_with(&self.path) {
            return None;
        }
        self.children
            .as_mut()?
            .iter_mut()
            .find_map(|child| child.find_mut(path))
    }

    pub(super) fn expand_at(&mut self, path: &Path) -> Result<(), ExplorerError> {
        match self.find_mut(path) {
            Some(node) => node.expand(),
            None => Ok(()),
        }
    }

    pub(super) fn collapse_at(&mut self, path: &Path) {
        if let Some(node) = self.find_mut(path) {
            node.expanded = false;
        }
    }

    /// Reads again the open directories in `scope`, keeping what is still
    /// there open. Closed directories are left alone: opening one reads it.
    /// An open directory that cannot be read any more is closed; only this
    /// node's own error is returned.
    pub(super) fn reload(&mut self, scope: Scope<'_>) -> Result<(), ExplorerError> {
        if self.kind != NodeKind::Dir || !self.expanded {
            return Ok(());
        }
        if scope.includes(&self.path) {
            let fresh = read_children(&self.path)?;
            self.children = Some(self.merge(fresh));
        }
        for child in self.children.iter_mut().flatten() {
            if child.reload(scope).is_err() {
                child.children = None;
                child.expanded = false;
            }
        }
        Ok(())
    }

    /// `fresh` (the directory as it is now) with what this node knew about the
    /// entries that are still there: whether they are open, and what they
    /// contain.
    fn merge(&mut self, fresh: Vec<Node>) -> Vec<Node> {
        let mut known: HashMap<PathBuf, Node> = self
            .children
            .take()
            .unwrap_or_default()
            .into_iter()
            .map(|node| (node.path.clone(), node))
            .collect();
        let mut merged = fresh;
        for node in &mut merged {
            let Some(previous) = known.remove(&node.path) else {
                continue;
            };
            node.expanded = previous.expanded && node.kind == NodeKind::Dir;
            node.children = previous.children;
        }
        merged
    }
    /// Appends this node and, if it is open, everything below it.
    pub(super) fn push_rows(&self, depth: usize, rows: &mut Vec<Row>) {
        rows.push(Row {
            depth,
            name: self.name.clone(),
            path: self.path.clone(),
            kind: self.kind,
            expanded: self.expanded,
        });
        if !self.expanded {
            return;
        }
        for child in self.children.iter().flatten() {
            child.push_rows(depth + 1, rows);
        }
    }
}

/// The entries of `dir`: directories first, then files, each in case
/// insensitive name order. Files the project's ignore rules (`.gitignore`,
/// `.ignore`, the global and per-repository excludes) exclude are left out.
fn read_children(dir: &Path) -> Result<Vec<Node>, ExplorerError> {
    let walker = WalkBuilder::new(dir)
        .max_depth(Some(1))
        .hidden(false)
        // Honor .gitignore files even outside a git repository.
        .require_git(false)
        .build();

    let mut children = Vec::new();
    for entry in walker {
        let entry = entry.map_err(|source| ExplorerError::Read {
            path: dir.to_path_buf(),
            source,
        })?;
        if entry.depth() == 0 || entry.file_name() == HIDDEN_DIRECTORY {
            continue;
        }
        children.push(Node::entry(entry.into_path()));
    }
    children.sort_by_cached_key(|node| {
        (
            Reverse(node.kind == NodeKind::Dir),
            node.name.to_lowercase(),
        )
    });
    Ok(children)
}
