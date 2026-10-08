//! The directory tree behind the explorer: nodes that load their children on
//! demand, and the flat list of rows the explorer shows.

use std::cmp::Reverse;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use thiserror::Error;

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

    /// Opens a directory, reading it the first time. Nothing changes if the
    /// read fails.
    fn expand(&mut self) -> Result<(), ExplorerError> {
        if self.kind == NodeKind::Dir && self.children.is_none() {
            self.children = Some(read_children(&self.path)?);
        }
        self.expanded = self.kind == NodeKind::Dir;
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

    /// Reads every directory that has been opened again, keeping what is
    /// still there open. A directory that cannot be read any more is closed;
    /// only the root's error is returned.
    pub(super) fn reload(&mut self) -> Result<(), ExplorerError> {
        let fresh = read_children(&self.path)?;
        self.children = Some(self.merge(fresh));
        Ok(())
    }

    fn merge(&mut self, fresh: Vec<Node>) -> Vec<Node> {
        let mut old = self.children.take().unwrap_or_default();
        let mut merged = Vec::with_capacity(fresh.len());
        for mut node in fresh {
            let previous = old.iter().position(|candidate| candidate.path == node.path);
            if let Some(index) = previous {
                let previous = old.swap_remove(index);
                node.expanded = previous.expanded && node.kind == NodeKind::Dir;
                node.children = previous.children;
            }
            if node.children.is_some() && node.reload().is_err() {
                node.children = None;
                node.expanded = false;
            }
            merged.push(node);
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
