//! Dragging a row onto a folder to move it.
//!
//! A press only selects the row; what a click does (open a file, open or
//! close a folder) waits for the release, so pressing to drag opens nothing. A
//! drag starts when the pointer leaves the row it was pressed on, and ends on
//! release: over a folder that can take the row it moves it, anywhere else it
//! does nothing.

use std::path::{Path, PathBuf};

use ratatui::layout::Position;

use super::{Explorer, ExplorerError, NodeKind};

/// The row a button went down on.
#[derive(Debug)]
pub(super) struct Press {
    path: PathBuf,
    /// The screen cell, to tell a drag from a click.
    row: u16,
}

/// A row being dragged, and the folder it would go into if let go now.
#[derive(Debug)]
pub(super) struct Drag {
    source: PathBuf,
    target: Option<PathBuf>,
}

/// What letting go of the button means.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Release {
    /// Nothing: a press outside the rows, or a drag dropped where it cannot go.
    Nothing,
    /// A click on a file: open it.
    Open(PathBuf),
    /// A drag dropped on a folder: move `from` into `into`.
    Move { from: PathBuf, into: PathBuf },
}

impl Explorer {
    /// The button went down on a screen cell: selects the row there, and
    /// remembers it in case this turns into a drag. False when there is no row.
    pub(crate) fn press(&mut self, column: u16, row: u16) -> bool {
        self.press = None;
        self.drag = None;
        let Some(index) = self.row_at(column, row) else {
            return false;
        };
        self.selected = index;
        self.follow_selection = true;
        self.press = Some(Press {
            path: self.rows[index].path.clone(),
            row,
        });
        true
    }

    /// Whether a button is held that went down on a row.
    pub(crate) fn is_pressed(&self) -> bool {
        self.press.is_some()
    }

    /// Whether a row is being dragged, as opposed to clicked.
    pub(crate) fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The folder the dragged row would go into, for highlighting it.
    pub(crate) fn drop_target(&self) -> Option<&Path> {
        self.drag.as_ref()?.target.as_deref()
    }

    /// The pointer moved with the button held.
    pub(crate) fn drag_to(&mut self, column: u16, row: u16) {
        let Some(press) = &self.press else {
            return;
        };
        if self.drag.is_none() {
            // The project folder itself cannot be moved.
            let is_root = self.root_path() == Some(press.path.as_path());
            if row == press.row || is_root {
                return;
            }
            self.drag = Some(Drag {
                source: press.path.clone(),
                target: None,
            });
        }
        let target = self.drop_folder_at(column, row);
        if let Some(drag) = &mut self.drag {
            drag.target = target;
        }
    }

    /// The button came up at a screen cell.
    pub(crate) fn release(&mut self, column: u16, row: u16) -> Result<Release, ExplorerError> {
        let Some(press) = self.press.take() else {
            return Ok(Release::Nothing);
        };
        // A terminal that sends no drag events still shows the pointer ended up
        // on another row, which is a drag.
        if self.drag.is_none() && row != press.row && self.root_path() != Some(press.path.as_path())
        {
            self.drag = Some(Drag {
                source: press.path.clone(),
                target: None,
            });
        }
        let Some(drag) = self.drag.take() else {
            return self
                .click(column, row)
                .map(|opened| opened.map_or(Release::Nothing, Release::Open));
        };
        Ok(match self.folder_for(&drag.source, column, row) {
            Some(into) => Release::Move {
                from: drag.source,
                into,
            },
            None => Release::Nothing,
        })
    }

    /// Gives up a drag in progress.
    pub(crate) fn cancel_drag(&mut self) {
        self.press = None;
        self.drag = None;
    }

    fn drop_folder_at(&self, column: u16, row: u16) -> Option<PathBuf> {
        let source = &self.drag.as_ref()?.source;
        self.folder_for(source, column, row)
    }

    /// The folder a row dropped on the cell would go into: the folder under the
    /// pointer, the folder a file is in, or the project folder for empty space.
    /// `None` outside the explorer, and where the move would do nothing or is
    /// not allowed (into itself or what it holds, into where it already is, into
    /// a link that loops).
    fn folder_for(&self, source: &Path, column: u16, row: u16) -> Option<PathBuf> {
        if !self.contains(column, row) {
            return None;
        }
        let folder = match self.row_at(column, row) {
            Some(index) => {
                let hit = &self.rows[index];
                match hit.kind {
                    NodeKind::Dir if hit.looped => return None,
                    NodeKind::Dir => hit.path.clone(),
                    NodeKind::File => hit.path.parent()?.to_path_buf(),
                }
            }
            None => self.root_path()?.to_path_buf(),
        };
        let already_there = source.parent() == Some(folder.as_path());
        (!already_there && !folder.starts_with(source)).then_some(folder)
    }

    /// The index of the row at a screen cell.
    fn row_at(&self, column: u16, row: u16) -> Option<usize> {
        let position = Position::new(column, row);
        self.body
            .contains(position)
            .then(|| self.scroll + usize::from(row - self.body.y))
            .filter(|&index| index < self.rows.len())
    }
}
