//! Documents with unsaved edits, for the recovery files, and a recovered text put back.

use std::path::Path;

use super::{Workspace, canonical};
use crate::buffer::{Buffer, Position};
use crate::persist::recovery::Recovery;

/// How a recovered text was put back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Opened {
    /// The file is open with the recovered text as an unsaved edit.
    InPlace,
    /// The text is in a new tab that has no file, so nothing on disk can be overwritten.
    AsCopy,
}

impl Workspace {
    /// What to keep of every document with unsaved edits, in the order they were opened.
    /// Documents the app locked are left out: they cannot have been edited.
    pub(crate) fn unsaved_for_recovery(&self, project: &Path) -> Vec<Recovery> {
        let mut ids: Vec<_> = self
            .documents
            .iter()
            .filter(|(_, doc)| doc.buffer.is_dirty() && !doc.buffer.is_locked())
            .map(|(id, _)| *id)
            .collect();
        ids.sort();
        ids.into_iter()
            .map(|id| {
                let buffer = &self.documents[&id].buffer;
                let head = self.cursor_in_document(id).unwrap_or_default();
                // A whole path: the next start may be in another folder than this one.
                let file = buffer.path().map(canonical);
                Recovery::new(project, file.as_deref(), buffer.text())
                    .with_title(buffer.title())
                    .with_disk(buffer.disk_fingerprint())
                    .with_cursor(head.line, head.column)
            })
            .collect()
    }

    /// Puts a recovered text in a tab in front. When the file is still as it was when the
    /// text was kept, the file's document gets the text as an unsaved edit; when the file
    /// changed, is gone or has unsaved edits of its own, the text goes to a new tab named
    /// "recovered: ..." that has no file.
    pub(crate) fn open_recovered(&mut self, item: &Recovery) -> Opened {
        let cursor = Position::new(item.line, item.column);
        if self.put_back_in_place(item, cursor) {
            return Opened::InPlace;
        }
        let mut buffer = Buffer::default();
        buffer.set_title(format!("recovered: {}", item.name()));
        // A new empty buffer takes any text.
        let _ = buffer.replace_all(&item.text);
        self.open_buffer(buffer);
        self.with_editor(|editor| editor.go_to(cursor));
        Opened::AsCopy
    }

    fn put_back_in_place(&mut self, item: &Recovery, cursor: Position) -> bool {
        let Some(file) = item.file.as_deref() else {
            return false;
        };
        let Ok(mut on_disk) = Buffer::open_or_new(file) else {
            return false;
        };
        if on_disk.disk_fingerprint() != item.disk {
            return false;
        }
        match self.document_of_path(file) {
            Some(document) => {
                let Some(doc) = self.documents.get(&document) else {
                    return false;
                };
                if doc.buffer.is_dirty() || doc.buffer.is_locked() {
                    return false;
                }
                let end = doc.buffer.len_bytes();
                let replaced = self.edit_document(document, |editor| {
                    editor.replace_ranges(&[(0..end, item.text.as_str())], 0, 0)
                });
                if !matches!(replaced, Some(Ok(()))) {
                    return false;
                }
                self.show_document(document);
            }
            None => {
                if on_disk.replace_all(&item.text).is_err() {
                    return false;
                }
                self.open_buffer(on_disk);
            }
        }
        self.with_editor(|editor| editor.go_to(cursor));
        true
    }

    /// The cursor of the first tab that shows `document`.
    fn cursor_in_document(&self, document: crate::ids::DocumentId) -> Option<Position> {
        self.panes
            .iter()
            .flat_map(|pane| &pane.tabs)
            .find(|tab| tab.document == document)
            .map(|tab| tab.view.selections().primary().head())
    }
}
