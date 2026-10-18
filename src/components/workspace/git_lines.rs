//! The marks in the gutter for lines that differ from git's index: which
//! documents need the index text, and keeping the differences up to date.

use std::path::PathBuf;

use super::Workspace;
use crate::git::Lines;
use crate::ids::DocumentId;

impl Workspace {
    /// Works out the marks of the documents that are on screen, if their text
    /// changed since. Run after the panes are laid out.
    pub(crate) fn refresh_git_lines(&mut self) {
        let shown: Vec<DocumentId> = self
            .panes
            .iter()
            .filter_map(|pane| pane.tabs.get(pane.active))
            .map(|tab| tab.document)
            .collect();
        for id in shown {
            if let Some(document) = self.documents.get_mut(&id) {
                document.git.refresh(&document.buffer);
            }
        }
    }

    /// The open documents that have a file, with its path.
    pub(crate) fn document_paths(&self) -> Vec<(DocumentId, PathBuf)> {
        self.documents
            .iter()
            .filter_map(|(id, doc)| Some((*id, doc.buffer.path()?.to_path_buf())))
            .collect()
    }

    /// The marks state of a document, for the one that asks for the index text
    /// and the one that takes its answer.
    pub(crate) fn git_lines_mut(&mut self, id: DocumentId) -> Option<&mut Lines> {
        Some(&mut self.documents.get_mut(&id)?.git)
    }

    /// Takes the marks off every document, for when git is not there any more.
    pub(crate) fn clear_git_lines(&mut self) {
        for document in self.documents.values_mut() {
            document.git.clear();
        }
    }
}
