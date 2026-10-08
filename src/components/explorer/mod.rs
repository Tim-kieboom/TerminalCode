//! The project tree: directories that open lazily, a selection, and the
//! commands that move it. Opening a file is left to the caller, which gets
//! the path back.

use std::path::{Path, PathBuf};

use ratatui::layout::Rect;
use serde::Deserialize;

use tree::Node;
pub(crate) use tree::{ExplorerError, NodeKind, Row};

mod render;
#[cfg(test)]
mod tests;
mod tree;

/// What the explorer can be asked to do with its selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ExplorerCommand {
    Up,
    Down,
    PageUp,
    PageDown,
    First,
    Last,
    /// Opens a closed directory, or steps into an open one.
    Expand,
    /// Closes an open directory, or steps out to the parent.
    Collapse,
    /// Opens or closes a directory; for a file, asks for it to be opened.
    Open,
    /// Reads the open directories again.
    Refresh,
}

#[derive(Debug, Default)]
pub(crate) struct Explorer {
    root: Option<Node>,
    rows: Vec<Row>,
    selected: usize,
    scroll: usize,
    /// Whether the next draw scrolls to keep the selection in view; the wheel
    /// turns it off so scrolling does not snap back.
    follow_selection: bool,
    /// Screen areas as of the last layout pass.
    area: Rect,
    body: Rect,
}

impl Explorer {
    /// An explorer showing the project in `root`.
    pub(crate) fn open(root: &Path) -> Result<Self, ExplorerError> {
        let mut explorer = Self {
            root: Some(Node::root(root)?),
            follow_selection: true,
            ..Self::default()
        };
        explorer.rebuild_rows();
        Ok(explorer)
    }

    /// The rows on screen, top to bottom.
    pub(crate) fn rows(&self) -> &[Row] {
        &self.rows
    }

    #[cfg(test)]
    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    pub(crate) fn selected_row(&self) -> Option<&Row> {
        self.rows.get(self.selected)
    }

    pub(crate) fn has_project(&self) -> bool {
        self.root.is_some()
    }

    #[cfg(test)]
    pub(crate) fn body_for_tests(&self) -> Rect {
        self.body
    }

    /// Whether a screen cell is inside the explorer.
    pub(crate) fn contains(&self, column: u16, row: u16) -> bool {
        self.area
            .contains(ratatui::layout::Position::new(column, row))
    }

    /// Does what `command` says. Returns the path of a file the user chose to
    /// open, if that is what happened.
    pub(crate) fn apply(
        &mut self,
        command: ExplorerCommand,
    ) -> Result<Option<PathBuf>, ExplorerError> {
        self.follow_selection = true;
        let last = self.rows.len().saturating_sub(1);
        let page = usize::from(self.body.height).saturating_sub(1).max(1);
        match command {
            ExplorerCommand::Up => self.selected = self.selected.saturating_sub(1),
            ExplorerCommand::Down => self.selected = (self.selected + 1).min(last),
            ExplorerCommand::PageUp => self.selected = self.selected.saturating_sub(page),
            ExplorerCommand::PageDown => self.selected = (self.selected + page).min(last),
            ExplorerCommand::First => self.selected = 0,
            ExplorerCommand::Last => self.selected = last,
            ExplorerCommand::Expand => self.expand_selected()?,
            ExplorerCommand::Collapse => self.collapse_selected(),
            ExplorerCommand::Open => return self.open_selected(),
            ExplorerCommand::Refresh => self.refresh()?,
        }
        Ok(None)
    }

    /// A click on a screen cell: selects the row there and opens it. Returns
    /// the file to open, if the row is a file.
    pub(crate) fn click(
        &mut self,
        column: u16,
        row: u16,
    ) -> Result<Option<PathBuf>, ExplorerError> {
        let position = ratatui::layout::Position::new(column, row);
        if !self.body.contains(position) {
            return Ok(None);
        }
        let index = self.scroll + usize::from(row - self.body.y);
        if index >= self.rows.len() {
            return Ok(None);
        }
        self.selected = index;
        self.follow_selection = true;
        self.open_selected()
    }

    /// Scrolls by `lines` (negative is up) without moving the selection.
    pub(crate) fn scroll_by(&mut self, lines: isize) {
        self.follow_selection = false;
        self.scroll = self
            .scroll
            .saturating_add_signed(lines)
            .min(self.max_scroll());
    }

    fn max_scroll(&self) -> usize {
        self.rows
            .len()
            .saturating_sub(usize::from(self.body.height))
    }

    fn expand_selected(&mut self) -> Result<(), ExplorerError> {
        let Some(row) = self.selected_row().cloned() else {
            return Ok(());
        };
        if row.kind != NodeKind::Dir {
            return Ok(());
        }
        if row.expanded {
            // Step into the directory.
            let first_child = self.selected + 1;
            let is_child = self
                .rows
                .get(first_child)
                .is_some_and(|next| next.depth > row.depth);
            if is_child {
                self.selected = first_child;
            }
            return Ok(());
        }
        self.expand_path(&row.path)
    }

    fn collapse_selected(&mut self) {
        let Some(row) = self.selected_row().cloned() else {
            return;
        };
        if row.kind == NodeKind::Dir && row.expanded {
            self.collapse_path(&row.path);
            return;
        }
        // Step out to the parent directory.
        let parent = self.rows[..self.selected]
            .iter()
            .rposition(|candidate| candidate.depth < row.depth);
        if let Some(parent) = parent {
            self.selected = parent;
        }
    }

    fn open_selected(&mut self) -> Result<Option<PathBuf>, ExplorerError> {
        let Some(row) = self.selected_row().cloned() else {
            return Ok(None);
        };
        match (row.kind, row.expanded) {
            (NodeKind::File, _) => Ok(Some(row.path)),
            (NodeKind::Dir, true) => {
                self.collapse_path(&row.path);
                Ok(None)
            }
            (NodeKind::Dir, false) => self.expand_path(&row.path).map(|()| None),
        }
    }

    fn refresh(&mut self) -> Result<(), ExplorerError> {
        let Some(root) = self.root.as_mut() else {
            return Ok(());
        };
        let result = root.reload();
        self.rebuild_keeping_selection();
        result
    }

    fn expand_path(&mut self, path: &Path) -> Result<(), ExplorerError> {
        let result = match self.root.as_mut() {
            Some(root) => root.expand_at(path),
            None => Ok(()),
        };
        self.rebuild_keeping_selection();
        result
    }

    fn collapse_path(&mut self, path: &Path) {
        if let Some(root) = self.root.as_mut() {
            root.collapse_at(path);
        }
        self.rebuild_keeping_selection();
    }

    /// Recomputes the rows after the tree changed, leaving the selection on
    /// the same path if it is still listed.
    fn rebuild_keeping_selection(&mut self) {
        let selected = self.selected_row().map(|row| row.path.clone());
        self.rebuild_rows();
        let Some(path) = selected else {
            return;
        };
        let position = self.rows.iter().position(|row| row.path == path);
        self.selected = position.unwrap_or(self.selected.min(self.rows.len().saturating_sub(1)));
    }

    fn rebuild_rows(&mut self) {
        self.rows.clear();
        if let Some(root) = &self.root {
            root.push_rows(0, &mut self.rows);
        }
    }
}
