//! Editing through the active tab, and keeping the other views of the document in step.

use super::{EditEvent, Workspace};
use crate::buffer::{EditInfo, Selection, Selections};
use crate::components::editor::{Editor, EditorRef};

impl Workspace {
    /// The focused pane's active tab, for reading.
    pub(crate) fn active_editor(&self) -> EditorRef<'_> {
        let Some(tab) = self.active_tab() else {
            return EditorRef::new(&self.empty.0, &self.empty.1);
        };
        EditorRef::new(&self.documents[&tab.document].buffer, &tab.view)
    }

    /// Runs `f` on the focused pane's active editor. Edits it makes are
    /// replayed onto the other views of the same document.
    pub(crate) fn with_editor<R>(&mut self, f: impl FnOnce(&mut Editor) -> R) -> R {
        let index = self.focused_index();
        self.with_editor_in(index, f)
    }

    /// Runs `f` on the editor of the pane under a screen cell, if any,
    /// without moving focus (the wheel scrolls what it is over).
    pub(crate) fn with_editor_at<R>(
        &mut self,
        column: u16,
        row: u16,
        f: impl FnOnce(&mut Editor) -> R,
    ) -> Option<R> {
        let index = self.pane_at(column, row)?;
        if self.panes[index].tabs.is_empty() {
            return None;
        }
        Some(self.with_editor_in(index, f))
    }

    pub(super) fn with_editor_in<R>(&mut self, pane: usize, f: impl FnOnce(&mut Editor) -> R) -> R {
        self.reset_confirmations();
        if self.panes[pane].tabs.is_empty() {
            // Nothing to edit: the closure gets a throwaway editor.
            return f(&mut Editor::default());
        }
        let tab_index = self.panes[pane].active;
        let tab = &self.panes[pane].tabs[tab_index];
        let (document, view_id) = (tab.document, tab.id);
        let others = self.other_tabs(document, (pane, tab_index));

        let doc = self.document_mut(document);
        let buffer = std::mem::take(&mut doc.buffer);
        let (indent, last_editor) = (doc.indent, doc.last_editor);
        let mut view = std::mem::take(&mut self.panes[pane].tabs[tab_index].view);
        view.set_indent(indent);
        if last_editor != Some(view_id) {
            view.end_edit_run();
        }
        let mut editor = Editor::from_parts(view, buffer);

        let before = self.byte_selections(&editor, &others);
        let result = f(&mut editor);
        let log = editor.take_edit_log();
        if !log.is_empty() {
            self.replay_onto_others(&editor, before, &log);
        }
        self.edit_events.extend(log.iter().map(|info| {
            (
                document,
                EditEvent {
                    start: info.start_byte,
                    old_end: info.old_end_byte,
                    new_end: info.new_end_byte,
                },
            )
        }));

        let (view, buffer) = editor.into_parts();
        self.panes[pane].tabs[tab_index].view = view;
        let doc = self.document_mut(document);
        doc.buffer = buffer;
        if !log.is_empty() {
            doc.syntax.record_edits(&log);
        }
        if !log.is_empty() {
            doc.last_editor = Some(view_id);
        }
        result
    }

    /// The selection of each tab in `tabs` as byte offsets into the text
    /// `editor` currently holds.
    pub(super) fn byte_selections(
        &self,
        editor: &Editor,
        tabs: &[(usize, usize)],
    ) -> Vec<ByteSelection> {
        let buffer = editor.buffer();
        tabs.iter()
            .filter_map(|&(pane, tab)| {
                let selection = *self.panes[pane].tabs[tab].view.selections().primary();
                Some(ByteSelection {
                    pane,
                    tab,
                    anchor: buffer.position_to_byte(selection.anchor()).ok()?,
                    head: buffer.position_to_byte(selection.head()).ok()?,
                })
            })
            .collect()
    }

    pub(super) fn replay_onto_others(
        &mut self,
        editor: &Editor,
        before: Vec<ByteSelection>,
        log: &[EditInfo],
    ) {
        let buffer = editor.buffer();
        for selection in before {
            let anchor = log
                .iter()
                .fold(selection.anchor, |byte, info| info.remap_byte(byte));

            let head = log
                .iter()
                .fold(selection.head, |byte, info| info.remap_byte(byte));

            let (Ok(anchor), Ok(head)) = (
                buffer.byte_to_position(anchor),
                buffer.byte_to_position(head),
            ) else {
                continue;
            };

            let selections = Selections::single(Selection::new(anchor, head));
            self.panes[selection.pane].tabs[selection.tab]
                .view
                .follow_remote_edit(selections, buffer.version());
        }
    }
}

/// One view's selection as byte offsets, while the text is being edited.
#[derive(Debug, Clone, Copy)]
pub(super) struct ByteSelection {
    pane: usize,
    tab: usize,
    anchor: usize,
    head: usize,
}
