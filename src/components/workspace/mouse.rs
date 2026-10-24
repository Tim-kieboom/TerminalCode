//! Clicks in the text of a pane.

use super::{Workspace, contains};
use crate::components::editor::{Editor, EditorRef};
use crate::event::mouse::Clicks;
use crate::ids::DocumentId;

impl Workspace {
    /// A left button press at a screen cell: focuses the pane, switches to a
    /// clicked tab, or places the cursor in the pane's text.
    pub(crate) fn mouse_press(
        &mut self,
        column: u16,
        row: u16,
        extend: bool,
        clicks: Clicks,
    ) -> Result<(), crate::buffer::BufferError> {
        self.press_with(column, row, |editor| {
            editor.mouse_press(column, row, extend, clicks)
        })
    }

    /// Like [`Workspace::mouse_press`], but a press on the text adds a cursor
    /// there instead of moving the one there is.
    pub(crate) fn mouse_press_adding(
        &mut self,
        column: u16,
        row: u16,
        clicks: Clicks,
    ) -> Result<(), crate::buffer::BufferError> {
        self.press_with(column, row, |editor| {
            editor.mouse_add_cursor(column, row, clicks)
        })
    }

    /// The document and line whose marker (the last column of the gutter) is at a screen cell:
    /// a click there toggles a breakpoint instead of moving the cursor.
    pub(crate) fn gutter_marker_at(&self, column: u16, row: u16) -> Option<(DocumentId, usize)> {
        let pane = self.panes.get(self.pane_at(column, row)?)?;
        let tab = pane.tabs.get(pane.active)?;
        let lines = self.documents.get(&tab.document)?.buffer.len_lines();
        let line = tab.view.marker_line_at(column, row, lines)?;
        Some((tab.document, line))
    }

    pub(super) fn press_with(
        &mut self,
        column: u16,
        row: u16,
        on_text: impl FnOnce(&mut Editor) -> Result<(), crate::buffer::BufferError>,
    ) -> Result<(), crate::buffer::BufferError> {
        let Some(index) = self.pane_at(column, row) else {
            return Ok(());
        };
        self.focused = self.panes[index].id;

        // A marker for hidden tabs goes to the nearest hidden one on its side.
        if let Some(tab) = self.panes[index].hidden_tab_at(column, row) {
            self.reset_confirmations();
            self.panes[index].active = tab;
            return Ok(());
        }

        let pane = &self.panes[index];
        if let Some(tab) = pane.tab_at(column, row) {
            self.reset_confirmations();
            self.panes[index].active = tab;
            return Ok(());
        }
        if !contains(pane.editor_area, column, row) {
            return Ok(());
        }
        self.with_editor_in(index, on_text)
    }

    /// A right button press: focuses the pane under the pointer and, if that is
    /// on the text, puts the cursor there (unless it is inside the selection).
    /// Returns whether the press was on text, where a context menu belongs.
    pub(crate) fn context_press(
        &mut self,
        column: u16,
        row: u16,
    ) -> Result<bool, crate::buffer::BufferError> {
        let Some(index) = self.pane_at(column, row) else {
            return Ok(false);
        };
        self.focused = self.panes[index].id;
        if self.panes[index].tabs.is_empty()
            || !contains(self.panes[index].editor_area, column, row)
        {
            return Ok(false);
        }
        self.with_editor_in(index, |editor| editor.mouse_context(column, row))
    }

    /// The document and the byte under a screen cell of any pane (not only the
    /// focused one), and the text of that document; `None` outside the text of a
    /// pane. Changes nothing, so it can follow the pointer.
    pub(crate) fn text_at_cell(
        &self,
        column: u16,
        row: u16,
    ) -> Option<(DocumentId, usize, ropey::Rope)> {
        let pane = &self.panes[self.pane_at(column, row)?];
        let tab = pane.tabs.get(pane.active)?;
        if !contains(pane.editor_area, column, row) {
            return None;
        }
        let buffer = &self.documents.get(&tab.document)?.buffer;
        let byte = EditorRef::new(buffer, &tab.view).byte_at_cell(column, row)?;
        Some((tab.document, byte, buffer.snapshot()))
    }

    /// A middle button press on the text of a pane: focuses it and puts the
    /// cursor there. Returns whether it was on text.
    pub(crate) fn place_cursor_at(
        &mut self,
        column: u16,
        row: u16,
    ) -> Result<bool, crate::buffer::BufferError> {
        let Some(index) = self.pane_at(column, row) else {
            return Ok(false);
        };
        self.focused = self.panes[index].id;
        if self.panes[index].tabs.is_empty()
            || !contains(self.panes[index].editor_area, column, row)
        {
            return Ok(false);
        }
        self.with_editor_in(index, |editor| editor.mouse_place_cursor(column, row))
    }
}
