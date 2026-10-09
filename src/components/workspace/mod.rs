//! Documents, panes and tabs.
//!
//! Each file is a [`Document`] stored once, by id. A pane is a strip of tabs;
//! each tab is a view (cursor, scroll, ...) of one document, so the same
//! document can be open in several tabs and panes. Panes are arranged in a
//! split tree.
//!
//! Editing goes through [`Workspace::with_editor`]: the document's buffer is
//! lent to an [`Editor`] together with the tab's view, and afterwards the
//! edits that were made are replayed onto the other views of the same
//! document so their cursors keep pointing at the same text.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ratatui::layout::Rect;
use serde::Deserialize;

use crate::buffer::{Buffer, DiskChange, EditInfo, FileError, Selection, Selections};
use crate::components::editor::{Editor, EditorRef, IndentStyle, ViewState, display_name};
use crate::components::quit_prompt::Item;
use crate::event::mouse::Clicks;
use crate::paths::canonical;
use crate::syntax::{DocumentSyntax, MAX_RANGES, merge_ranges};
use crate::ui::layout::Axis;
use crate::ui::theme::Theme;

pub(crate) use disk::{DiskEvent, SaveOutcome};
use tree::Node;

mod disk;
mod render;
#[cfg(test)]
mod tests;
mod tree;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct DocumentId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ViewId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PaneId(u32);

/// A direction to move focus between panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FocusDirection {
    Left,
    Right,
    Up,
    Down,
}

/// What closing a tab did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CloseResult {
    Closed,
    /// The tab holds the only view of a document with unsaved changes. Close
    /// again to discard them.
    Unsaved(String),
}

#[derive(Debug)]
struct Document {
    buffer: Buffer,
    indent: IndentStyle,
    /// The change on disk the user was already told about, so repeated file
    /// events do not repeat the warning.
    warned: Option<DiskChange>,
    /// The view that made the latest change, so a typing burst is not merged
    /// with another view's edit.
    last_editor: Option<ViewId>,
    /// Colors for the part of the text on screen.
    syntax: DocumentSyntax,
}

#[derive(Debug)]
struct Tab {
    id: ViewId,
    document: DocumentId,
    view: ViewState,
}

#[derive(Debug)]
struct Pane {
    id: PaneId,
    tabs: Vec<Tab>,
    active: usize,
    /// Screen areas as of the last layout pass; used for mouse hits and
    /// directional focus.
    area: Rect,
    tab_bar: Rect,
    tab_rects: Vec<Rect>,
    editor_area: Rect,
    /// The row of the find bar; empty when there is none.
    find_bar: Rect,
}

#[derive(Debug)]
pub(crate) struct Workspace {
    documents: HashMap<DocumentId, Document>,
    /// In creation order; `tree` gives the on-screen order.
    panes: Vec<Pane>,
    tree: Node,
    focused: PaneId,
    next_id: u32,
    confirm_close: Option<ViewId>,
    /// The document whose save was refused because its file changed on disk;
    /// saving it again overwrites the file.
    confirm_overwrite: Option<DocumentId>,
    /// Changes whenever a document is opened or closed.
    docs_version: u64,
    /// Whether the focused pane has a row reserved for the find bar.
    find_bar: bool,
    /// Stands in for the active editor while the only pane has no tabs.
    empty: (Buffer, ViewState),
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new(Editor::default())
    }
}

impl Workspace {
    /// A workspace with one pane holding one tab for `editor`.
    pub(crate) fn new(editor: Editor) -> Self {
        let pane = PaneId(0);
        let mut workspace = Self {
            documents: HashMap::new(),
            panes: Vec::new(),
            tree: Node::Leaf(pane),
            focused: pane,
            next_id: 1,
            confirm_close: None,
            confirm_overwrite: None,
            docs_version: 0,
            find_bar: false,
            empty: (Buffer::default(), ViewState::default()),
        };
        let (view, buffer) = editor.into_parts();
        let document = workspace.add_document(buffer);
        let tab = workspace.new_tab(document, view);
        workspace.panes.push(Pane::with_tab(pane, tab));
        workspace
    }

    /// Brings the colors of every document that is on screen up to date, for
    /// the lines each pane shows and a screen of margin above and below, so a
    /// scroll does not show uncolored text. Run after the panes are laid out.
    ///
    /// Returns what went wrong for documents whose highlighting could not be
    /// set up, each once.
    pub(crate) fn refresh_highlights(&mut self, theme: &Theme) -> Vec<String> {
        let mut errors = Vec::new();
        let mut wanted: HashMap<DocumentId, Vec<std::ops::Range<usize>>> = HashMap::new();
        for pane in &self.panes {
            let Some(tab) = pane.tabs.get(pane.active) else {
                continue;
            };
            let buffer = &self.documents[&tab.document].buffer;
            let height = tab.view.viewport_height();
            let top = tab.view.scroll().top;
            let first = buffer.line_start_byte(top.saturating_sub(height));
            let last = buffer.line_start_byte(top + 2 * height);
            wanted.entry(tab.document).or_default().push(first..last);
        }
        for (id, ranges) in wanted {
            let ranges = merge_ranges(ranges, MAX_RANGES);
            if let Some(document) = self.documents.get_mut(&id) {
                document.syntax.update(&document.buffer, theme, &ranges);
                if let Some(error) = document.syntax.take_error() {
                    errors.push(format!("{}: {error}", display_name(&document.buffer)));
                }
            }
        }
        errors
    }

    /// The focused pane's active tab, for reading.
    pub(crate) fn active_editor(&self) -> EditorRef<'_> {
        let Some(tab) = self.active_tab() else {
            return EditorRef::new(&self.empty.0, &self.empty.1);
        };
        EditorRef::new(&self.documents[&tab.document].buffer, &tab.view)
    }

    /// Reserves (or gives back) a row at the bottom of the focused pane for
    /// the find bar.
    pub(crate) fn set_find_bar(&mut self, shown: bool) {
        self.find_bar = shown;
    }

    /// Whether the focused pane has a tab. After the last tab is closed the
    /// editor area is empty until a new file is opened.
    pub(crate) fn has_tabs(&self) -> bool {
        !self.focused_pane().tabs.is_empty()
    }

    /// Number of panes.
    #[cfg(test)]
    pub(crate) fn pane_count(&self) -> usize {
        self.panes.len()
    }

    /// Screen areas of the panes (creation order) as of the last layout pass.
    #[cfg(test)]
    pub(crate) fn pane_areas(&self) -> Vec<Rect> {
        self.panes.iter().map(|pane| pane.area).collect()
    }

    #[cfg(test)]
    pub(crate) fn focused_area(&self) -> Rect {
        self.focused_pane().area
    }

    /// Screen areas of the focused pane's tabs as of the last layout pass.
    #[cfg(test)]
    pub(crate) fn tab_areas(&self) -> Vec<Rect> {
        self.focused_pane().tab_rects.clone()
    }

    /// Number of open documents.
    #[cfg(test)]
    pub(crate) fn document_count(&self) -> usize {
        self.documents.len()
    }

    /// Tab names of the focused pane, and which one is active.
    #[cfg(test)]
    pub(crate) fn tab_names(&self) -> (Vec<String>, usize) {
        let pane = self.focused_pane();
        let names = pane
            .tabs
            .iter()
            .map(|tab| display_name(&self.documents[&tab.document].buffer))
            .collect();
        (names, pane.active)
    }

    /// Runs `f` on the focused pane's active editor. Edits it makes are
    /// replayed onto the other views of the same document.
    pub(crate) fn with_editor<R>(&mut self, f: impl FnOnce(&mut Editor) -> R) -> R {
        let index = self.focused_index();
        self.with_editor_in(index, f)
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

    fn find_tab_of(&self, document: DocumentId) -> Option<(usize, usize)> {
        self.panes.iter().enumerate().find_map(|(pane, p)| {
            let tab = p.tabs.iter().position(|tab| tab.document == document)?;
            Some((pane, tab))
        })
    }

    /// Forgets the pending "close again" and "save again" confirmations; any
    /// other action in between cancels them.
    fn reset_confirmations(&mut self) {
        self.confirm_close = None;
        self.confirm_overwrite = None;
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
        let wanted = canonical(path);
        let open = self.documents.iter().find_map(|(id, doc)| {
            let own = doc.buffer.path()?;
            (canonical(own) == wanted).then_some(*id)
        });
        let Some(document) = open else {
            self.open_buffer(Buffer::open(path)?);
            return Ok(());
        };
        self.show_document(document);
        Ok(())
    }

    /// Activates a tab of `document`, preferring the focused pane; opens a new
    /// tab for it in the focused pane if no pane shows it.
    fn show_document(&mut self, document: DocumentId) {
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

    /// Closes the active tab. Closing the last tab of a pane closes the pane;
    /// closing the last tab of the last pane leaves the editor area empty.
    pub(crate) fn close_tab(&mut self) -> CloseResult {
        let pane = self.focused_index();
        if self.panes[pane].tabs.is_empty() {
            return CloseResult::Closed;
        }
        let tab = self.panes[pane].active;
        self.close_tab_at(pane, tab)
    }

    /// A middle button press: closes the tab under the pointer, if it is on a
    /// tab. Like [`Workspace::close_tab`], but for any tab and without moving
    /// focus.
    pub(crate) fn middle_press(&mut self, column: u16, row: u16) -> Option<CloseResult> {
        let pane = self.pane_at(column, row)?;
        let tab = self.panes[pane].tab_at(column, row)?;
        Some(self.close_tab_at(pane, tab))
    }

    /// Closes tab `tab` of pane `pane` (indices into `panes` and its tabs).
    fn close_tab_at(&mut self, index: usize, tab_index: usize) -> CloseResult {
        let Some(tab) = self.panes[index].tabs.get(tab_index) else {
            return CloseResult::Closed;
        };
        let (tab_id, document) = (tab.id, tab.document);

        let shared = self.views_of(document) > 1;
        let doc = &self.documents[&document];
        if doc.buffer.is_dirty() && !shared && self.confirm_close != Some(tab_id) {
            self.confirm_close = Some(tab_id);
            return CloseResult::Unsaved(display_name(&doc.buffer));
        }

        self.remove_tab(index, tab_index);
        CloseResult::Closed
    }

    /// Removes a tab without asking. The document goes with its last view.
    fn remove_tab(&mut self, index: usize, tab_index: usize) {
        let Some(tab) = self.panes[index].tabs.get(tab_index) else {
            return;
        };
        let document = tab.document;
        let shared = self.views_of(document) > 1;
        self.reset_confirmations();
        let pane = &mut self.panes[index];
        pane.tabs.remove(tab_index);
        pane.active = if tab_index < pane.active {
            pane.active - 1
        } else {
            pane.active.min(pane.tabs.len().saturating_sub(1))
        };
        if !shared {
            self.documents.remove(&document);
            self.docs_version += 1;
        }
        if self.panes[index].tabs.is_empty() {
            self.remove_pane(index);
        }
    }

    /// Activates tab `index` of the focused pane (the last one if out of range).
    pub(crate) fn activate_tab(&mut self, index: usize) {
        self.reset_confirmations();
        let pane = self.focused_pane_mut();
        pane.active = index.min(pane.tabs.len().saturating_sub(1));
    }

    pub(crate) fn next_tab(&mut self) {
        self.step_tab(1);
    }

    pub(crate) fn previous_tab(&mut self) {
        self.step_tab(-1);
    }

    /// Splits the focused pane. The new pane, which gets the focus, shows the
    /// same document as the active tab.
    pub(crate) fn split(&mut self, axis: Axis) {
        self.reset_confirmations();
        let Some(source) = self.active_tab() else {
            return;
        };
        let (document, view) = (source.document, source.view.duplicate());

        let tab = self.new_tab(document, view);
        let id = PaneId(self.take_id());
        self.tree.split(self.focused, axis, id);
        self.panes.push(Pane::with_tab(id, tab));
        self.focused = id;
    }

    /// Moves focus to the next pane in reading order, wrapping around.
    pub(crate) fn focus_next_pane(&mut self) {
        self.reset_confirmations();
        let order = self.tree.leaves();
        let Some(position) = order.iter().position(|id| *id == self.focused) else {
            return;
        };
        self.focused = order[(position + 1) % order.len()];
    }

    /// Moves focus to the nearest pane in `direction`, if there is one. Uses
    /// the screen areas of the last layout pass.
    pub(crate) fn focus_direction(&mut self, direction: FocusDirection) {
        self.reset_confirmations();
        let from = self.focused_pane().area;
        let best = self
            .panes
            .iter()
            .filter(|pane| pane.id != self.focused)
            .filter_map(|pane| Some((pane.id, neighbor_score(from, pane.area, direction)?)))
            .min_by_key(|(_, score)| *score);
        if let Some((id, _)) = best {
            self.focused = id;
        }
    }

    /// A left button press at a screen cell: focuses the pane, switches to a
    /// clicked tab, or places the cursor in the pane's text.
    pub(crate) fn mouse_press(
        &mut self,
        column: u16,
        row: u16,
        extend: bool,
        clicks: Clicks,
    ) -> Result<(), crate::buffer::BufferError> {
        let Some(index) = self.pane_at(column, row) else {
            return Ok(());
        };
        self.focused = self.panes[index].id;

        let pane = &self.panes[index];
        if let Some(tab) = pane.tab_at(column, row) {
            self.reset_confirmations();
            self.panes[index].active = tab;
            return Ok(());
        }
        if !contains(pane.editor_area, column, row) {
            return Ok(());
        }
        self.with_editor_in(index, |editor| {
            editor.mouse_press(column, row, extend, clicks)
        })
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

    fn with_editor_in<R>(&mut self, pane: usize, f: impl FnOnce(&mut Editor) -> R) -> R {
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

        let (view, buffer) = editor.into_parts();
        self.panes[pane].tabs[tab_index].view = view;
        let doc = self.document_mut(document);
        doc.buffer = buffer;
        if !log.is_empty() {
            doc.last_editor = Some(view_id);
        }
        result
    }

    /// Positions of every other tab showing `document`, except `skip`.
    fn other_tabs(&self, document: DocumentId, skip: (usize, usize)) -> Vec<(usize, usize)> {
        let mut others = Vec::new();
        for (p, pane) in self.panes.iter().enumerate() {
            for (t, tab) in pane.tabs.iter().enumerate() {
                if tab.document == document && (p, t) != skip {
                    others.push((p, t));
                }
            }
        }
        others
    }

    /// The selection of each tab in `tabs` as byte offsets into the text
    /// `editor` currently holds.
    fn byte_selections(&self, editor: &Editor, tabs: &[(usize, usize)]) -> Vec<ByteSelection> {
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

    fn replay_onto_others(
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

    fn add_document(&mut self, buffer: Buffer) -> DocumentId {
        let id = DocumentId(self.take_id());
        let indent = IndentStyle::detect(&buffer).unwrap_or_default();
        self.docs_version += 1;
        self.documents.insert(
            id,
            Document {
                buffer,
                indent,
                last_editor: None,
                warned: None,
                syntax: DocumentSyntax::default(),
            },
        );
        id
    }

    fn new_tab(&mut self, document: DocumentId, view: ViewState) -> Tab {
        Tab {
            id: ViewId(self.take_id()),
            document,
            view,
        }
    }

    fn take_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn views_of(&self, document: DocumentId) -> usize {
        self.panes
            .iter()
            .flat_map(|pane| &pane.tabs)
            .filter(|tab| tab.document == document)
            .count()
    }

    fn step_tab(&mut self, delta: isize) {
        self.reset_confirmations();
        let pane = self.focused_pane_mut();
        let count = pane.tabs.len() as isize;
        if count == 0 {
            return;
        }
        pane.active = (pane.active as isize + delta).rem_euclid(count) as usize;
    }

    /// Drops the pane at `index` after its last tab was closed. The only pane
    /// is kept, empty.
    fn remove_pane(&mut self, index: usize) {
        if self.panes.len() == 1 {
            self.panes[0].active = 0;
            return;
        }

        let id = self.panes[index].id;
        self.tree.remove(id);
        self.panes.remove(index);
        if self.focused == id {
            let neighbor = index.saturating_sub(1).min(self.panes.len() - 1);
            self.focused = self.panes[neighbor].id;
        }
    }

    fn pane_at(&self, column: u16, row: u16) -> Option<usize> {
        self.panes
            .iter()
            .position(|pane| contains(pane.area, column, row))
    }

    fn focused_index(&self) -> usize {
        self.panes
            .iter()
            .position(|pane| pane.id == self.focused)
            .unwrap_or(0)
    }

    fn focused_pane(&self) -> &Pane {
        &self.panes[self.focused_index()]
    }

    fn focused_pane_mut(&mut self) -> &mut Pane {
        let index = self.focused_index();
        &mut self.panes[index]
    }

    fn active_tab(&self) -> Option<&Tab> {
        let pane = self.focused_pane();
        pane.tabs.get(pane.active)
    }

    fn document_mut(&mut self, id: DocumentId) -> &mut Document {
        self.documents
            .get_mut(&id)
            .expect("every tab refers to an open document")
    }
}

impl Pane {
    /// The tab drawn at a screen cell, going by the last layout pass. Tabs can
    /// have been closed since, so a rectangle left over from the last frame
    /// does not count if its tab is gone.
    fn tab_at(&self, column: u16, row: u16) -> Option<usize> {
        let tab = self
            .tab_rects
            .iter()
            .position(|rect| contains(*rect, column, row))?;

        (tab < self.tabs.len()).then_some(tab)
    }

    fn with_tab(id: PaneId, tab: Tab) -> Self {
        Self {
            id,
            tabs: vec![tab],
            active: 0,
            area: Rect::default(),
            tab_bar: Rect::default(),
            tab_rects: Vec::new(),
            editor_area: Rect::default(),
            find_bar: Rect::default(),
        }
    }
}

/// One view's selection as byte offsets, while the text is being edited.
#[derive(Debug, Clone, Copy)]
struct ByteSelection {
    pane: usize,
    tab: usize,
    anchor: usize,
    head: usize,
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    area.contains(ratatui::layout::Position::new(column, row))
}

/// How good a neighbor `other` is for moving from `from` in `direction`:
/// smaller is nearer, and `None` if it is not in that direction or does not
/// overlap `from` on the other axis.
fn neighbor_score(from: Rect, other: Rect, direction: FocusDirection) -> Option<(u32, u32)> {
    let (gap, overlap_start, overlap_end, center_gap) = match direction {
        FocusDirection::Left => (
            i32::from(from.x) - i32::from(other.x + other.width),
            from.y.max(other.y),
            (from.y + from.height).min(other.y + other.height),
            center_distance(from.y, from.height, other.y, other.height),
        ),
        FocusDirection::Right => (
            i32::from(other.x) - i32::from(from.x + from.width),
            from.y.max(other.y),
            (from.y + from.height).min(other.y + other.height),
            center_distance(from.y, from.height, other.y, other.height),
        ),
        FocusDirection::Up => (
            i32::from(from.y) - i32::from(other.y + other.height),
            from.x.max(other.x),
            (from.x + from.width).min(other.x + other.width),
            center_distance(from.x, from.width, other.x, other.width),
        ),
        FocusDirection::Down => (
            i32::from(other.y) - i32::from(from.y + from.height),
            from.x.max(other.x),
            (from.x + from.width).min(other.x + other.width),
            center_distance(from.x, from.width, other.x, other.width),
        ),
    };
    if gap < 0 || overlap_start >= overlap_end {
        return None;
    }
    Some((gap as u32, center_gap))
}

fn center_distance(start_a: u16, len_a: u16, start_b: u16, len_b: u16) -> u32 {
    let center_a = u32::from(start_a) * 2 + u32::from(len_a);
    let center_b = u32::from(start_b) * 2 + u32::from(len_b);
    center_a.abs_diff(center_b)
}
