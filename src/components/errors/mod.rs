//! The Errors view: what the language servers say is wrong with the open
//! documents, worst first, one row each. It is a view of a bar (the bottom one
//! in the default layout) and has its own keys and selection.

use std::path::Path;

use ratatui::layout::Rect;
use serde::Deserialize;

use crate::buffer::Position;
use crate::components::definition_list::short_path;
use crate::components::workspace::DocumentId;
use crate::lsp::Severity;

mod render;
#[cfg(test)]
mod tests;

/// What the Errors view can be asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ErrorsCommand {
    Up,
    Down,
    PageUp,
    PageDown,
    First,
    Last,
    /// Opens the file of the selected row at its place.
    Open,
    /// Gives the keyboard back to the editor.
    Leave,
}

/// One problem in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ErrorRow {
    pub(crate) document: DocumentId,
    /// The file, short (relative to the project when it is in it).
    pub(crate) file: String,
    /// Where it is, from 0.
    pub(crate) position: Position,
    pub(crate) severity: Severity,
    /// The first line of what the server said.
    pub(crate) message: String,
}

impl ErrorRow {
    pub(crate) fn new(
        document: DocumentId,
        path: Option<&Path>,
        name: &str,
        project: Option<&Path>,
        position: Position,
        severity: Severity,
        message: &str,
    ) -> Self {
        let file = match (path, project) {
            (Some(path), Some(project)) => short_path(path, project),
            (Some(path), None) => path.display().to_string(),
            (None, _) => name.to_owned(),
        };
        Self {
            document,
            file,
            position,
            severity,
            message: message.lines().next().unwrap_or_default().to_owned(),
        }
    }

    /// `file:line:column`, counted from 1.
    pub(crate) fn place(&self) -> String {
        format!(
            "{}:{}:{}",
            self.file,
            self.position.line + 1,
            self.position.column + 1
        )
    }
}

/// Orders the rows: errors first, then warnings and the rest, each group by file
/// and then by place in it.
pub(crate) fn sort(rows: &mut [ErrorRow]) {
    rows.sort_by(|a, b| (a.severity, &a.file, a.position).cmp(&(b.severity, &b.file, b.position)));
}

/// The selection and the scroll of the list; the rows themselves are made from
/// the diagnostics at every draw.
#[derive(Debug, Default)]
pub(crate) struct ErrorsView {
    selected: usize,
    scroll: usize,
    /// Whether the next draw scrolls to keep the selection in view; the wheel
    /// turns it off so scrolling does not snap back.
    follow_selection: bool,
    area: Rect,
    body: Rect,
}

impl ErrorsView {
    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    #[cfg(test)]
    pub(crate) fn scroll(&self) -> usize {
        self.scroll
    }

    pub(crate) fn area(&self) -> Rect {
        self.area
    }

    /// Where the rows are drawn, as of the last draw.
    #[cfg(test)]
    pub(crate) fn body(&self) -> Rect {
        self.body
    }

    /// Whether a screen cell is inside the view.
    pub(crate) fn contains(&self, column: u16, row: u16) -> bool {
        self.area
            .contains(ratatui::layout::Position::new(column, row))
    }

    /// Moves the selection, keeping it among `count` rows.
    pub(crate) fn apply(&mut self, command: ErrorsCommand, count: usize) {
        let last = count.saturating_sub(1);
        let page = usize::from(self.body.height).saturating_sub(1).max(1);
        self.selected = match command {
            ErrorsCommand::Up => self.selected.saturating_sub(1),
            ErrorsCommand::Down => (self.selected + 1).min(last),
            ErrorsCommand::PageUp => self.selected.saturating_sub(page),
            ErrorsCommand::PageDown => (self.selected + page).min(last),
            ErrorsCommand::First => 0,
            ErrorsCommand::Last => last,
            ErrorsCommand::Open | ErrorsCommand::Leave => self.selected.min(last),
        };
        self.follow_selection = true;
    }

    /// Scrolls by `rows` (negative is up) without moving the selection.
    pub(crate) fn scroll_by(&mut self, rows: isize, count: usize) {
        self.follow_selection = false;
        let top = self.scroll.saturating_add_signed(rows);
        self.scroll = top.min(self.max_scroll(count));
    }

    /// Selects the row at screen row `row`, if there is one. Returns whether it did.
    pub(crate) fn select_at(&mut self, column: u16, row: u16, count: usize) -> bool {
        let inside = self
            .body
            .contains(ratatui::layout::Position::new(column, row));
        let index = self.scroll + usize::from(row.saturating_sub(self.body.y));
        if !inside || index >= count {
            return false;
        }
        self.selected = index;
        self.follow_selection = true;
        true
    }

    /// Takes where it is drawn and keeps the selection and the scroll sensible
    /// for `count` rows.
    pub(crate) fn prepare(&mut self, area: Rect, body: Rect, count: usize) {
        self.area = area;
        self.body = body;
        self.selected = self.selected.min(count.saturating_sub(1));
        self.scroll = self.scroll.min(self.max_scroll(count));
        if !self.follow_selection {
            return;
        }
        let height = usize::from(body.height);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if height > 0 && self.selected >= self.scroll + height {
            self.scroll = self.selected + 1 - height;
        }
    }

    fn max_scroll(&self, count: usize) -> usize {
        count.saturating_sub(usize::from(self.body.height).max(1))
    }
}
