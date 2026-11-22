//! The folder browser of the Open File and Open Folder prompts: the explorer's tree, rooted at the
//! folder being typed, in the place of the list. Every move of the selection writes its path as
//! the name, so the prompt stays what it is (a path) and leaving the browser goes on typing from
//! there. What starts and ends it, and what to do with a chosen path, is the app's
//! (`app/popups/new_entry.rs`).

use std::path::{Path, PathBuf};

use ratatui::Frame;
use ratatui::layout::Rect;

use super::{NamePrompt, Purpose};
use crate::components::explorer::{Explorer, ExplorerCommand, ExplorerError, NodeKind};
use crate::ui::theme::Theme;

/// How many rows the browser shows.
pub(super) const BROWSE_ROWS: u16 = 12;

impl NamePrompt {
    /// Whether this prompt has a browser at all: the ones that ask for a path to open.
    pub(crate) fn can_browse(&self) -> bool {
        matches!(self.purpose, Purpose::OpenFile | Purpose::OpenFolder)
    }

    pub(crate) fn browsing(&self) -> bool {
        self.browse.is_some()
    }

    /// Shows the tree of `root` in the place of the list. Open Folder lists folders only, and
    /// both leave out the names that start with a dot until `.` shows them.
    pub(crate) fn start_browsing(&mut self, root: &Path) -> Result<(), ExplorerError> {
        let mut explorer = Explorer::open(root)?;
        explorer.set_dirs_only(self.purpose == Purpose::OpenFolder);
        explorer.apply(ExplorerCommand::ToggleHidden)?;
        self.browse = Some(Box::new(explorer));
        self.write_selected();
        Ok(())
    }

    /// Goes back to the list; the name is the path the browser ended on.
    pub(crate) fn stop_browsing(&mut self) {
        self.browse = None;
    }

    /// Moves the selection of the browser, opens or closes a folder, or shows the dot names. A
    /// step out of the folder the tree started at goes to the folder above it.
    pub(crate) fn browse_do(&mut self, command: ExplorerCommand) -> Result<(), ExplorerError> {
        let Some(explorer) = self.browse.as_mut() else {
            return Ok(());
        };
        let at_root = explorer.selected_row().is_some_and(|row| row.depth == 0);
        let parent = explorer
            .root_path()
            .and_then(Path::parent)
            .map(Path::to_path_buf);
        match (command, at_root, parent) {
            (ExplorerCommand::Collapse, true, Some(parent)) => self.browse_above(&parent)?,
            _ => {
                explorer.apply(command)?;
            }
        }
        self.write_selected();
        Ok(())
    }

    /// The tree again, from `parent`, with the folder it was of selected.
    fn browse_above(&mut self, parent: &Path) -> Result<(), ExplorerError> {
        let Some(old) = self
            .browse
            .as_ref()
            .and_then(|e| e.root_path())
            .map(Path::to_path_buf)
        else {
            return Ok(());
        };
        let mut explorer = Explorer::open(parent)?;
        explorer.set_dirs_only(self.purpose == Purpose::OpenFolder);
        explorer.apply(ExplorerCommand::ToggleHidden)?;
        explorer.reveal(&old)?;
        self.browse = Some(Box::new(explorer));
        Ok(())
    }

    /// `enter` in the browser: whether a path was chosen. A folder is the choice for Open
    /// Folder; for Open File it opens or closes, and a file is the choice.
    pub(crate) fn browse_enter(&mut self) -> Result<bool, ExplorerError> {
        let Some(explorer) = self.browse.as_mut() else {
            return Ok(false);
        };
        let Some(row) = explorer.selected_row().cloned() else {
            return Ok(false);
        };
        match (row.kind, &self.purpose) {
            (NodeKind::File, _) | (NodeKind::Dir, Purpose::OpenFolder) => Ok(true),
            _ => {
                explorer.apply(ExplorerCommand::Open)?;
                Ok(false)
            }
        }
    }

    /// The system's file dialog is about to open. `false` when one is open already.
    pub(crate) fn begin_dialog(&mut self) -> bool {
        !std::mem::replace(&mut self.dialog_open, true)
    }

    /// The dialog is closed. Whether this prompt was waiting for one.
    pub(crate) fn end_dialog(&mut self) -> bool {
        std::mem::replace(&mut self.dialog_open, false)
    }

    /// Takes `path` as the name, as the browser does, for a path the dialog gave.
    pub(crate) fn set_path(&mut self, path: &Path) {
        let text = path.to_string_lossy().into_owned();
        self.field.replace_from(0, &text);
    }

    /// Whether a screen cell is on the browser.
    pub(crate) fn browse_contains(&self, column: u16, row: u16) -> bool {
        self.browsing()
            && self
                .layout
                .list
                .contains(ratatui::layout::Position::new(column, row))
    }

    /// A click on a row: selects it and, as in the explorer, opens or closes a folder. Whether
    /// a file was chosen.
    pub(crate) fn browse_click(&mut self, column: u16, row: u16) -> Result<bool, ExplorerError> {
        let Some(explorer) = self.browse.as_mut() else {
            return Ok(false);
        };
        let chosen = explorer.click(column, row)?.is_some();
        self.write_selected();
        Ok(chosen)
    }

    /// The wheel: scrolls the tree by `rows` (negative is up).
    pub(crate) fn browse_scroll(&mut self, rows: isize) {
        if let Some(explorer) = self.browse.as_mut() {
            explorer.scroll_by(rows);
        }
    }

    /// The wheel over the box: scrolls the tree, or the list, by `rows` (negative is up).
    /// Whether the pointer was over the box.
    pub(crate) fn scroll_over(&mut self, column: u16, row: u16, rows: isize) -> bool {
        if !self
            .layout
            .area
            .contains(ratatui::layout::Position::new(column, row))
        {
            return false;
        }
        match self.browsing() {
            true => self.browse_scroll(rows),
            false => self.scroll_list(rows),
        }
        true
    }

    /// Writes the path of the selected row as the name (a folder with a separator after it).
    fn write_selected(&mut self) {
        let Some(row) = self.browse.as_ref().and_then(|e| e.selected_row()) else {
            return;
        };
        let mut text: PathBuf = row.path.clone();
        if row.kind == NodeKind::Dir {
            text.push("");
        }
        let text = text.to_string_lossy().into_owned();
        self.field.replace_from(0, &text);
    }

    pub(super) fn render_browse(&self, frame: &mut Frame, theme: &Theme, area: Rect) {
        if let Some(explorer) = self.browse.as_ref() {
            explorer.render_rows(frame, theme, area);
        }
    }
}
