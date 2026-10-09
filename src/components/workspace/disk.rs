//! Keeping documents in step with their files: noticing that a file changed
//! under an open document, and refusing to overwrite such a change by accident.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::{DocumentId, Workspace, canonical};
use crate::buffer::{Buffer, DiskChange, FileError, Selection, Selections};

/// What was done about a document whose file changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DiskEvent {
    /// The document had no edits of its own and took the new contents.
    Reloaded(String),
    /// The file changed but the document has unsaved edits. They are kept;
    /// saving will ask before overwriting the file.
    Conflict(String),
    /// The file is gone. The document keeps its text.
    Deleted(String),
    /// The file could not be read.
    Unreadable(String),
}

/// What saving a document did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SaveOutcome {
    Saved,
    /// The file changed on disk since the document read it, so nothing was
    /// written. Saving again overwrites it.
    NeedsConfirmation(String),
}

impl Workspace {
    /// Looks at every document whose file is in `changed` and brings it in
    /// step: reloads it if it has no edits, flags a conflict if it has.
    /// A change already reported is not reported again.
    pub(crate) fn check_disk(&mut self, changed: &[PathBuf]) -> Vec<DiskEvent> {
        let changed: Vec<PathBuf> = changed.iter().map(|path| canonical(path)).collect();
        let mut affected: Vec<DocumentId> = self
            .documents
            .iter()
            .filter(|(_, doc)| {
                doc.buffer
                    .path()
                    .is_some_and(|path| changed.contains(&canonical(path)))
            })
            .map(|(id, _)| *id)
            .collect();
        affected.sort();
        affected
            .into_iter()
            .filter_map(|id| self.check_document(id))
            .collect()
    }

    fn check_document(&mut self, id: DocumentId) -> Option<DiskEvent> {
        let doc = self.documents.get_mut(&id)?;
        let name = file_name(&doc.buffer);
        let change = match doc.buffer.disk_change() {
            Ok(change) => change,
            Err(error) => return Some(DiskEvent::Unreadable(error.to_string())),
        };
        let Some(change) = change else {
            doc.warned = None;
            return None;
        };
        if doc.warned == Some(change) {
            return None;
        }
        match change {
            DiskChange::Deleted => {
                doc.warned = Some(change);
                Some(DiskEvent::Deleted(name))
            }
            DiskChange::Modified(_) if doc.buffer.is_dirty() => {
                doc.warned = Some(change);
                Some(DiskEvent::Conflict(name))
            }
            DiskChange::Modified(_) => match self.reload_document(id) {
                Ok(()) => Some(DiskEvent::Reloaded(name)),
                Err(error) => Some(DiskEvent::Unreadable(error.to_string())),
            },
        }
    }

    /// Replaces a document's text with its file and moves every view's cursor
    /// to the nearest place that still exists (the line and column it was on,
    /// clamped).
    fn reload_document(&mut self, id: DocumentId) -> Result<(), FileError> {
        let Some(doc) = self.documents.get_mut(&id) else {
            return Ok(());
        };
        doc.buffer.reload()?;
        doc.warned = None;
        doc.last_editor = None;

        let buffer = &doc.buffer;
        for tab in self.panes.iter_mut().flat_map(|pane| pane.tabs.iter_mut()) {
            if tab.document != id {
                continue;
            }
            let selection = *tab.view.selections().primary();
            let clamped = Selection::new(
                buffer.clamp_position(selection.anchor()),
                buffer.clamp_position(selection.head()),
            );
            tab.view
                .follow_remote_edit(Selections::single(clamped), buffer.version());
        }
        Ok(())
    }

    /// Writes document `id` to its file, unless the file changed on disk since
    /// the document read it: then nothing is written and the next save of the
    /// same document, with nothing else done in between, overwrites it. A
    /// document that is no longer open counts as saved.
    pub(crate) fn save_document(&mut self, id: DocumentId) -> Result<SaveOutcome, FileError> {
        let Some(doc) = self.documents.get_mut(&id) else {
            return Ok(SaveOutcome::Saved);
        };
        let confirmed = self.confirm_overwrite == Some(id);
        let changed = matches!(doc.buffer.disk_change()?, Some(DiskChange::Modified(_)));
        if changed && !confirmed {
            self.confirm_overwrite = Some(id);
            return Ok(SaveOutcome::NeedsConfirmation(file_name(&doc.buffer)));
        }
        self.confirm_overwrite = None;
        doc.buffer.save()?;
        doc.warned = None;
        Ok(SaveOutcome::Saved)
    }

    /// [`Workspace::save_document`] for the focused pane's active tab.
    pub(crate) fn save_active(&mut self) -> Result<SaveOutcome, FileError> {
        let Some(tab) = self.active_tab() else {
            return Ok(SaveOutcome::Saved);
        };
        let id = tab.document;
        self.save_document(id)
    }

    /// The directories that hold the open documents' files, without
    /// duplicates; the places to watch for outside changes.
    pub(crate) fn directories(&self) -> Vec<PathBuf> {
        let dirs: BTreeSet<PathBuf> = self
            .documents
            .values()
            .filter_map(|doc| doc.buffer.path())
            .map(|path| {
                canonical(path)
                    .parent()
                    .map_or_else(|| Path::new(".").to_path_buf(), Path::to_path_buf)
            })
            .collect();
        dirs.into_iter().collect()
    }

    /// Changes whenever a document is opened or closed.
    pub(crate) fn documents_version(&self) -> u64 {
        self.docs_version
    }
}

/// The file's name, without the modified marker the tab shows.
fn file_name(buffer: &Buffer) -> String {
    buffer.path().and_then(Path::file_name).map_or_else(
        || "[no name]".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}
