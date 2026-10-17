//! Opening, finding and closing documents, and what is known about them.

use super::{Document, EditEvent, Workspace};
use crate::buffer::{Buffer, FileError};
use crate::components::editor::{IndentPrefs, ViewState, display_name};
use crate::components::popups::quit_prompt::Item;
use crate::files::paths::canonical;
use crate::ids::DocumentId;
use crate::syntax::DocumentSyntax;
use std::path::{Path, PathBuf};

impl Workspace {
    /// Number of open documents.
    #[cfg(test)]
    pub(crate) fn document_count(&self) -> usize {
        self.documents.len()
    }

    /// Every document with unsaved changes, in the order they were opened.
    pub(crate) fn dirty_documents(&self) -> Vec<Item> {
        let mut dirty: Vec<_> = self
            .documents
            .iter()
            .filter(|(_, doc)| doc.buffer.is_dirty())
            .map(|(id, doc)| Item {
                document: *id,
                name: display_name(&doc.buffer),
            })
            .collect();
        dirty.sort_by_key(|item| item.document);
        dirty
    }

    /// The open documents whose file is `path` or inside it.
    pub(crate) fn documents_under(&self, path: &Path) -> Vec<DocumentId> {
        self.documents_with_suffix(path)
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    }

    /// The open documents whose file is `path` or inside it, each with where it
    /// is below `path` (empty for `path` itself).
    pub(crate) fn documents_with_suffix(&self, path: &Path) -> Vec<(DocumentId, PathBuf)> {
        let root = canonical(path);
        let mut found: Vec<_> = self
            .documents
            .iter()
            .filter_map(|(id, doc)| {
                let own = canonical(doc.buffer.path()?);
                let suffix = own.strip_prefix(&root).ok()?;
                Some((*id, suffix.to_path_buf()))
            })
            .collect();
        found.sort();
        found
    }

    /// Points documents at the new place of their files after a move.
    pub(crate) fn set_document_paths(&mut self, moves: Vec<(DocumentId, PathBuf)>) {
        for (id, path) in moves {
            if let Some(doc) = self.documents.get_mut(&id) {
                doc.buffer.set_path(path);
            }
        }
        self.docs_version += 1;
    }

    /// The documents among `documents` that have unsaved changes.
    pub(crate) fn dirty_among(&self, documents: &[DocumentId]) -> Vec<Item> {
        self.dirty_documents()
            .into_iter()
            .filter(|item| documents.contains(&item.document))
            .collect()
    }

    /// Closes every tab of `documents` without asking, e.g. because their
    /// files were deleted.
    pub(crate) fn close_documents(&mut self, documents: &[DocumentId]) {
        for document in documents {
            while let Some((pane, tab)) = self.find_tab_of(*document) {
                self.remove_tab(pane, tab);
            }
        }
    }

    /// Opens `buffer` in a new tab of the focused pane and activates it.
    pub(crate) fn open_buffer(&mut self, buffer: Buffer) {
        self.reset_confirmations();
        let document = self.add_document(buffer);
        let tab = self.new_tab(document, ViewState::default());
        let pane = self.focused_pane_mut();
        pane.tabs.push(tab);
        pane.active = pane.tabs.len() - 1;
    }

    /// Shows the file at `path`: switches to a tab that already shows it (in
    /// this pane first, then any other), or opens it in a new tab of the
    /// focused pane. A file that is open is never read a second time, so two
    /// tabs can not disagree about its contents.
    pub(crate) fn open_path(&mut self, path: &Path) -> Result<(), FileError> {
        let open = self.document_of_path(path);
        let Some(document) = open else {
            self.open_buffer(Buffer::open(path)?);
            self.remember_recent(path);
            return Ok(());
        };
        self.show_document(document);
        self.remember_recent(path);
        Ok(())
    }

    /// The open document whose file is `path`, if there is one.
    pub(crate) fn document_of_path(&self, path: &Path) -> Option<DocumentId> {
        let wanted = canonical(path);
        self.documents.iter().find_map(|(id, doc)| {
            let own = doc.buffer.path()?;
            (canonical(own) == wanted).then_some(*id)
        })
    }

    /// What a save as root needs of a document: its file, its text, the version of
    /// the text, and whether the app locked it for good (then it is never saved).
    pub(crate) fn root_save_source(
        &self,
        document: DocumentId,
    ) -> Option<(PathBuf, String, u64, bool)> {
        let buffer = &self.documents.get(&document)?.buffer;
        Some((
            buffer.path()?.to_path_buf(),
            buffer.text(),
            buffer.version(),
            buffer.is_locked(),
        ))
    }

    /// Notes that a save as root wrote `text` to the file of `document`.
    pub(crate) fn mark_written_as_root(&mut self, document: DocumentId, text: &str, version: u64) {
        if let Some(doc) = self.documents.get_mut(&document) {
            doc.buffer.wrote_elsewhere(text, version);
        }
    }

    /// Lets the text of the document in front, a file read-only to the user, be
    /// edited. Whether it can be edited now.
    pub(crate) fn unlock_active_for_root(&mut self) -> bool {
        let Some(document) = self.active_document_id() else {
            return false;
        };
        let Some(doc) = self.documents.get_mut(&document) else {
            return false;
        };
        let editable = doc.buffer.unlock_for_root();
        doc.read_only = doc.buffer.is_read_only();
        editable
    }

    /// Locks the text of `document` against edits for good (see [`Buffer::lock`]).
    pub(crate) fn lock_document(&mut self, document: DocumentId) {
        if let Some(doc) = self.documents.get_mut(&document) {
            doc.buffer.lock();
            doc.read_only = true;
        }
    }

    /// Activates a tab of `document`, preferring the focused pane; opens a new
    /// tab for it in the focused pane if no pane shows it.
    pub(crate) fn show_document(&mut self, document: DocumentId) {
        self.reset_confirmations();
        let focused = self.focused_index();
        let order =
            std::iter::once(focused).chain((0..self.panes.len()).filter(|&index| index != focused));
        for index in order {
            let Some(tab) = self.panes[index]
                .tabs
                .iter()
                .position(|tab| tab.document == document)
            else {
                continue;
            };
            self.focused = self.panes[index].id;
            self.panes[index].active = tab;
            return;
        }
        let tab = self.new_tab(document, ViewState::default());
        let pane = self.focused_pane_mut();
        pane.tabs.push(tab);
        pane.active = pane.tabs.len() - 1;
    }

    /// An empty untitled document in a new tab.
    pub(crate) fn new_file(&mut self) {
        self.open_buffer(Buffer::default());
    }

    /// The files of the open documents.
    pub(crate) fn open_paths(&self) -> Vec<PathBuf> {
        self.documents
            .values()
            .filter_map(|document| document.buffer.path().map(Path::to_path_buf))
            .collect()
    }

    /// The file, the name for a title and the text of an open document.
    pub(crate) fn document_info(
        &self,
        document: DocumentId,
    ) -> Option<(Option<&Path>, String, &Buffer)> {
        let buffer = &self.documents.get(&document)?.buffer;
        Some((buffer.path(), display_name(buffer), buffer))
    }

    /// Whether the document is open in a tab.
    pub(crate) fn has_document(&self, document: DocumentId) -> bool {
        self.documents.contains_key(&document)
    }

    /// The edits made since the last call, oldest first.
    pub(crate) fn take_edit_events(&mut self) -> Vec<(DocumentId, EditEvent)> {
        std::mem::take(&mut self.edit_events)
    }

    /// The version of a document's text, which changes with every edit.
    pub(crate) fn document_version(&self, document: DocumentId) -> Option<u64> {
        Some(self.documents.get(&document)?.buffer.version())
    }

    /// The version of the text of every open document.
    pub(crate) fn document_versions(&self) -> std::collections::HashMap<DocumentId, u64> {
        self.documents
            .iter()
            .map(|(id, doc)| (*id, doc.buffer.version()))
            .collect()
    }

    pub(super) fn add_document(&mut self, buffer: Buffer) -> DocumentId {
        let mut buffer = buffer;
        let read_only = buffer.sync_read_only();
        let id = DocumentId::new(self.take_id());
        let indent = self.indent_prefs.style_for(&buffer);
        self.docs_version += 1;
        self.documents.insert(
            id,
            Document {
                read_only,
                buffer,
                indent,
                last_editor: None,
                warned: None,
                saves: 0,
                reloads: 0,
                syntax: DocumentSyntax::new(u64::from(id.number())),
            },
        );
        id
    }

    /// Applies the indentation settings: how wide a tab character draws, in every
    /// pane and in the tabs opened from now on, and the indentation of every
    /// open document (its own style where detection is on and it shows one).
    pub(crate) fn set_indent_prefs(&mut self, prefs: IndentPrefs) {
        self.indent_prefs = prefs;
        self.empty.1.set_tab_width(prefs.size);
        for tab in self.panes.iter_mut().flat_map(|pane| pane.tabs.iter_mut()) {
            tab.view.set_tab_width(prefs.size);
        }
        for doc in self.documents.values_mut() {
            doc.indent = prefs.style_for(&doc.buffer);
        }
    }

    pub(super) fn document_mut(&mut self, id: DocumentId) -> &mut Document {
        self.documents
            .get_mut(&id)
            .expect("every tab refers to an open document")
    }
}
