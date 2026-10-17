//! Closing several tabs at once, remembering closed ones, and saving every
//! modified file.

use std::path::{Path, PathBuf};

use super::{DocumentId, SaveOutcome, ViewId, ViewState, Workspace, display_name};
use crate::buffer::{Buffer, FileError, Position};
use crate::files::paths::canonical;
use crate::ui::layout::Axis;

/// How many closed tabs are remembered.
const REMEMBERED_TABS: usize = 20;

/// Which tabs a bulk close covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloseScope {
    /// Every tab of the focused pane except the one showing.
    Others,
    /// Every tab of the focused pane to the left of the one showing.
    Left,
    /// Every tab of the focused pane to the right of the one showing.
    Right,
    /// Every tab of every pane.
    All,
}

/// What a bulk close did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CloseMany {
    /// This many tabs are closed.
    Closed(usize),
    /// These documents would lose unsaved changes. Nothing was closed; doing the
    /// same again, with nothing else done in between, discards them.
    Unsaved(Vec<String>),
}

/// A tab that was closed, for reopening it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ClosedTab {
    path: PathBuf,
    cursor: Position,
}

/// What saving every modified file did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct SaveAll {
    pub(crate) saved: usize,
    /// Modified documents that have no file yet.
    pub(crate) untitled: Vec<String>,
    /// Files that changed on disk since they were read; none of them was
    /// written.
    pub(crate) changed_on_disk: Vec<String>,
    /// Files that could not be written, with why.
    pub(crate) failed: Vec<(String, String)>,
}

impl Workspace {
    /// Closes the tabs `scope` covers. When that would discard unsaved changes
    /// nothing is closed the first time; the same call again does it.
    pub(crate) fn close_tabs(&mut self, scope: CloseScope) -> CloseMany {
        let doomed = self.doomed_tabs(scope);
        if doomed.is_empty() {
            return CloseMany::Closed(0);
        }

        let unsaved = self.unsaved_among(&doomed);
        if !unsaved.is_empty() && self.confirm_bulk != Some(scope) {
            self.confirm_bulk = Some(scope);
            return CloseMany::Unsaved(unsaved);
        }

        let mut closed = 0;
        for view in doomed {
            // Closing a tab can close its pane, so places are looked up anew.
            if let Some((pane, tab)) = self.find_view(view) {
                self.close_remembering(pane, tab);
                closed += 1;
            }
        }
        CloseMany::Closed(closed)
    }

    /// The tabs a bulk close covers; a pinned tab is never among them.
    fn doomed_tabs(&self, scope: CloseScope) -> Vec<ViewId> {
        let in_focused = |wanted: &dyn Fn(usize, usize) -> bool| {
            let pane = self.focused_pane();
            pane.tabs
                .iter()
                .enumerate()
                .filter(|(index, tab)| !tab.pinned && wanted(*index, pane.active))
                .map(|(_, tab)| tab.id)
                .collect()
        };
        match scope {
            CloseScope::All => self
                .panes
                .iter()
                .flat_map(|pane| pane.tabs.iter())
                .filter(|tab| !tab.pinned)
                .map(|tab| tab.id)
                .collect(),
            CloseScope::Others => in_focused(&|index, active| index != active),
            CloseScope::Left => in_focused(&|index, active| index < active),
            CloseScope::Right => in_focused(&|index, active| index > active),
        }
    }

    /// The names of the modified documents all of whose tabs are among `doomed`,
    /// which would lose their changes.
    fn unsaved_among(&self, doomed: &[ViewId]) -> Vec<String> {
        let mut lost: Vec<(DocumentId, String)> = Vec::new();
        for tab in self.panes.iter().flat_map(|pane| &pane.tabs) {
            if !doomed.contains(&tab.id) || lost.iter().any(|(id, _)| *id == tab.document) {
                continue;
            }
            let survives = self
                .panes
                .iter()
                .flat_map(|pane| &pane.tabs)
                .any(|other| other.document == tab.document && !doomed.contains(&other.id));
            let buffer = &self.documents[&tab.document].buffer;
            if !survives && buffer.is_dirty() {
                lost.push((tab.document, display_name(buffer)));
            }
        }
        lost.sort_by_key(|(id, _)| *id);
        lost.into_iter().map(|(_, name)| name).collect()
    }

    fn find_view(&self, view: ViewId) -> Option<(usize, usize)> {
        self.panes.iter().enumerate().find_map(|(pane, p)| {
            let tab = p.tabs.iter().position(|tab| tab.id == view)?;
            Some((pane, tab))
        })
    }

    /// Removes a tab without asking, and remembers it for reopening when it was
    /// the last view of a file.
    pub(super) fn close_remembering(&mut self, pane: usize, tab_index: usize) {
        if let Some(tab) = self.panes[pane].tabs.get(tab_index) {
            let shared = self.views_of(tab.document) > 1;
            let buffer = &self.documents[&tab.document].buffer;
            if let (false, Some(path)) = (shared, buffer.path()) {
                let cursor = tab.view.selections().primary().head();
                self.closed.retain(|closed| closed.path != path);
                self.closed.push(ClosedTab {
                    path: path.to_path_buf(),
                    cursor,
                });
                let extra = self.closed.len().saturating_sub(REMEMBERED_TABS);
                self.closed.drain(..extra);
            }
        }
        self.remove_tab(pane, tab_index);
    }

    /// Opens the file of the tab closed last, with the cursor where it was.
    /// `None` when no closed tab is remembered; an error when the file cannot be
    /// opened any more (it is forgotten either way).
    pub(crate) fn reopen_closed_tab(&mut self) -> Option<Result<PathBuf, FileError>> {
        let closed = self.closed.pop()?;
        if let Err(error) = self.open_path(&closed.path) {
            return Some(Err(error));
        }
        self.with_editor(|editor| {
            let at = editor.buffer().clamp_position(closed.cursor);
            editor.go_to(at);
        });
        Some(Ok(closed.path))
    }

    /// How many closed tabs can be reopened.
    #[cfg(test)]
    pub(crate) fn closed_tab_count(&self) -> usize {
        self.closed.len()
    }

    /// Saves every modified document that has a file. A file that changed on
    /// disk is left alone, and so is a document with no file yet.
    pub(crate) fn save_all(&mut self) -> SaveAll {
        let mut report = SaveAll::default();
        for item in self.dirty_documents() {
            let Some(doc) = self.documents.get(&item.document) else {
                continue;
            };
            if doc.buffer.path().is_none() {
                report.untitled.push(item.name);
                continue;
            }
            match self.save_document(item.document) {
                Ok(SaveOutcome::Saved) => report.saved += 1,
                Ok(SaveOutcome::NeedsConfirmation(name)) => report.changed_on_disk.push(name),
                Ok(SaveOutcome::FileDeleted(name)) => report.failed.push((
                    name,
                    "its file was deleted on disk; save it on its own to recreate it".to_owned(),
                )),
                Err(error) => report.failed.push((item.name, error.to_string())),
            }
        }
        // A refused save arms an overwrite on the next save; saving all must
        // not leave that armed for a file the user has not looked at.
        self.confirm_overwrite = None;
        report
    }
}

/// How many recently opened files are remembered.
const REMEMBERED_FILES: usize = 20;

impl Workspace {
    /// Notes that the file at `path` was opened, for [`Workspace::recent_paths`].
    pub(super) fn remember_recent(&mut self, path: &Path) {
        self.recent.retain(|known| known != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(REMEMBERED_FILES);
    }

    /// The files opened lately, the latest first, as they were opened.
    pub(crate) fn recent_paths(&self) -> &[PathBuf] {
        &self.recent
    }

    /// Shows the file at `path` in a new pane beside or below the focused one,
    /// which gets the keyboard. A file that cannot be read changes nothing. With
    /// no tab to split, it opens like any other.
    pub(crate) fn open_path_in_split(&mut self, path: &Path, axis: Axis) -> Result<(), FileError> {
        if !self.has_tabs() {
            return self.open_path(path);
        }
        // A file not open yet is read before the split, so a failure leaves the
        // panes as they were.
        let wanted = canonical(path);
        let open = self.documents.iter().find_map(|(id, doc)| {
            let own = doc.buffer.path()?;
            (canonical(own) == wanted).then_some(*id)
        });
        let document = match open {
            Some(document) => document,
            None => self.add_document(Buffer::open(path)?),
        };

        // The split starts with a copy of the active tab; the file takes its place.
        self.split(axis);
        let pane = self.focused_index();
        let tab = self.new_tab(document, ViewState::default());
        self.panes[pane].tabs.push(tab);
        self.panes[pane].active = self.panes[pane].tabs.len() - 1;
        self.remove_tab(pane, 0);
        self.remember_recent(path);
        Ok(())
    }
}
