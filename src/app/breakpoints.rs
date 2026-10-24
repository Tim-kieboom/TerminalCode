//! Breakpoints in the editor: toggled on the line of the cursor (`F9`) or with a click on the
//! marker column of the gutter, kept on their lines through edits, shown in the gutter, and
//! remembered for the next run (see `trcode-breakpoints`, which keeps them, and "Run
//! configurations and a debugger" in `docs/design.md`). Telling a debug adapter about them is
//! for a debug session.

use std::path::{Path, PathBuf};

use super::App;
use super::plugin_trust::trust_key;
use crate::breakpoints::{Change, Saved, Text};
use crate::buffer::Buffer;
use crate::files::paths::canonical;
use crate::ids::DocumentId;
use crate::persist::saved_state::SavedBreakpoint;

/// A buffer, as the breakpoints read its text.
struct BufferText<'a>(&'a Buffer);

impl Text for BufferText<'_> {
    fn line_count(&self) -> usize {
        self.0.len_lines()
    }

    fn line_start(&self, line: usize) -> usize {
        self.0.line_start_byte(line)
    }

    fn line_of(&self, byte: usize) -> usize {
        self.0
            .byte_to_position(byte)
            .map(|position| position.line)
            .unwrap_or_default()
    }

    fn line_text(&self, line: usize) -> String {
        self.0
            .line_content(line)
            .map(std::borrow::Cow::into_owned)
            .unwrap_or_default()
    }
}

impl App {
    /// `F9`: sets a breakpoint on the line of the cursor, or takes the one there away.
    pub(super) fn toggle_breakpoint(&mut self) {
        let Some(document) = self.state.workspace().active_document_id() else {
            return;
        };
        let line = self.state.editor().selections().primary().head().line;
        self.toggle_breakpoint_in(document, line);
    }

    /// Sets or takes away the breakpoint on `line` of the file open as `document`.
    pub(super) fn toggle_breakpoint_in(&mut self, document: DocumentId, line: usize) {
        let Some((Some(path), _, _)) = self.state.workspace().document_info(document) else {
            self.state
                .notify("a breakpoint needs a file: save this one first");
            return;
        };
        let path = canonical(path);
        let mut points = std::mem::take(self.state.breakpoints_mut());
        let set = self
            .state
            .workspace()
            .document_info(document)
            .map(|(_, _, buffer)| points.toggle(&path, document, &BufferText(buffer), line));
        *self.state.breakpoints_mut() = points;
        if set.is_some() {
            self.remember_breakpoints();
            self.needs_redraw = true;
        }
    }

    /// "Debug: Remove All Breakpoints".
    pub(super) fn clear_breakpoints(&mut self) {
        let count = self.state.breakpoints_mut().clear();
        match count {
            0 => self.state.notify("there are no breakpoints"),
            1 => self.state.notify("removed 1 breakpoint"),
            count => self.state.notify(format!("removed {count} breakpoints")),
        }
        self.remember_breakpoints();
        self.needs_redraw = true;
    }

    /// The edits made to the documents since the last call move the breakpoints (and the places
    /// of the jump list, which `changes` is for too). Returns them.
    pub(super) fn apply_edit_events(&mut self) -> Vec<(DocumentId, Change)> {
        let events = self.state.workspace_mut().take_edit_events();
        let changes: Vec<(DocumentId, Change)> = events
            .into_iter()
            .map(|(document, event)| {
                let change = Change {
                    start: event.start,
                    old_end: event.old_end,
                    new_end: event.new_end,
                };
                (document, change)
            })
            .collect();
        if changes.is_empty() || self.state.breakpoints().attached().is_empty() {
            return changes;
        }
        let mut points = std::mem::take(self.state.breakpoints_mut());
        let mut documents: Vec<DocumentId> =
            changes.iter().map(|(document, _)| *document).collect();
        documents.dedup();
        for document in documents {
            let batch: Vec<Change> = changes
                .iter()
                .filter(|(of, _)| *of == document)
                .map(|(_, change)| *change)
                .collect();
            if let Some((_, _, buffer)) = self.state.workspace().document_info(document) {
                points.remap(document, &batch, &BufferText(buffer));
            }
        }
        *self.state.breakpoints_mut() = points;
        self.needs_redraw = true;
        changes
    }

    /// Files that were opened get their breakpoints (those the file has, put on its lines),
    /// and those that were closed let go of them. Called after every input and event.
    pub(super) fn sync_breakpoint_documents(&mut self) {
        if self.state.breakpoints().is_empty() && self.state.breakpoints().attached().is_empty() {
            return;
        }
        let mut points = std::mem::take(self.state.breakpoints_mut());
        let mut changed = false;
        for document in points.attached() {
            if !self.state.workspace().has_document(document) {
                points.detach(document);
                changed = true;
            }
        }
        for (document, path) in self.state.workspace().documents_with_paths() {
            let path = canonical(&path);
            if points.path_of(document) == Some(path.as_path()) {
                continue;
            }
            if let Some((_, _, buffer)) = self.state.workspace().document_info(document) {
                points.attach(&path, document, &BufferText(buffer));
                changed = true;
            }
        }
        *self.state.breakpoints_mut() = points;
        if changed {
            self.remember_breakpoints();
            self.needs_redraw = true;
        }
    }

    /// Takes in the breakpoints remembered for this project.
    pub(super) fn import_saved_breakpoints(&mut self) {
        let project = trust_key(&self.project_root());
        let root = self.project_root();
        let saved: Vec<Saved> = self
            .saved_breakpoints(&project)
            .into_iter()
            .map(|item| Saved {
                path: absolute(&root, &item.file),
                line: item.line,
                text: item.text,
            })
            .collect();
        self.state.breakpoints_mut().import(saved);
    }

    /// Writes the breakpoints to `state.json` when they are not what is there.
    pub(super) fn remember_breakpoints(&mut self) {
        let project = trust_key(&self.project_root());
        let root = canonical(&self.project_root());
        let items: Vec<SavedBreakpoint> = self
            .state
            .breakpoints()
            .export()
            .into_iter()
            .map(|saved| SavedBreakpoint {
                file: relative(&root, &saved.path),
                line: saved.line,
                text: saved.text,
            })
            .collect();
        self.store_breakpoints(&project, items);
    }

    /// The breakpoints, for the debug session.
    #[cfg(test)]
    pub(crate) fn breakpoints_of(&self, path: &Path) -> Vec<usize> {
        self.state.breakpoints().lines(path)
    }
}

/// `path` from `root` when it is inside it, else as it is.
fn relative(root: &Path, path: &Path) -> String {
    let shown = path.strip_prefix(root).unwrap_or(path);
    shown.to_string_lossy().replace('\\', "/")
}

/// The file a remembered `file` is: from `root` when it is not a whole path.
fn absolute(root: &Path, file: &str) -> PathBuf {
    let path = PathBuf::from(file);
    match path.is_absolute() {
        true => path,
        false => canonical(&root.join(path)),
    }
}
