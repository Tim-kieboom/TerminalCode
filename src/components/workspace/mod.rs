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
use std::path::PathBuf;

use ratatui::layout::Rect;
use serde::Deserialize;

use crate::buffer::{Buffer, DiskChange};
use crate::components::editor::{Editor, IndentPrefs, IndentStyle, ViewState, display_name};
use crate::files::paths::canonical;
use crate::ids::{DocumentId, PaneId, ViewId};
use crate::syntax::DocumentSyntax;
use crate::syntax::worker::Key as SyntaxKey;

use bulk::ClosedTab;
pub(crate) use bulk::{CloseMany, CloseScope};
pub(crate) use disk::{DiskEvent, LspDocument, SaveOutcome};
pub(crate) use panes::neighbor_score;
pub(crate) use tree::Divider;
use tree::Node;

mod bulk;
mod disk;
mod documents;
mod editing;
mod git_lines;
mod highlights;
mod mouse;
mod panes;
mod render;
mod tabs;
#[cfg(test)]
mod tests;
mod tree;

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
    /// How many times the document was saved, for a language server to be told.
    saves: u64,
    /// How many times the text was read again from the file.
    reloads: u64,
    /// Whether the file was read-only when last looked at, for the tab's note.
    read_only: bool,
    /// The view that made the latest change, so a typing burst is not merged
    /// with another view's edit.
    last_editor: Option<ViewId>,
    /// Colors for the part of the text on screen.
    syntax: DocumentSyntax,
    /// Which lines differ from the file in git's index.
    git: crate::git::Lines,
}

#[derive(Debug)]
struct Tab {
    id: ViewId,
    document: DocumentId,
    view: ViewState,
    /// Pinned tabs sit at the left of the strip, are left alone by the close-many
    /// items and need an explicit close.
    pinned: bool,
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
    /// The "‹3" and "2›" that say how many tabs are scrolled out of the bar on
    /// each side; empty when none are.
    overflow: (Rect, Rect),
    /// How many tabs are out of the bar before the first and after the last.
    hidden: (usize, usize),
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
    /// The bulk close that asked for confirmation; doing it again confirms.
    confirm_bulk: Option<CloseScope>,
    /// The document whose save was refused because its file changed on disk;
    /// saving it again overwrites the file.
    confirm_overwrite: Option<DocumentId>,
    /// Changes whenever a document is opened or closed.
    docs_version: u64,
    /// Syntax keys of closed documents, for the worker to drop what it keeps.
    forgotten: Vec<SyntaxKey>,
    /// Tabs closed lately, the latest last, for reopening them.
    closed: Vec<ClosedTab>,
    /// Files opened lately, the latest first.
    recent: Vec<PathBuf>,
    /// How many rows the focused pane has reserved at its bottom for the find
    /// bar: none, the find row, or that and the replace row.
    find_bar: u16,
    /// Stands in for the active editor while the only pane has no tabs.
    empty: (Buffer, ViewState),
    /// What the settings say about indentation and the width of a tab character,
    /// for every document and view, existing and new.
    indent_prefs: IndentPrefs,
    /// The edits made since they were last taken, for whatever keeps places in
    /// the text (the jump list): the document, and the bytes `start..old_end` that became `start..new_end`.
    edit_events: Vec<(DocumentId, EditEvent)>,
}

/// One edit of a document, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EditEvent {
    pub(crate) start: usize,
    pub(crate) old_end: usize,
    pub(crate) new_end: usize,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new(Editor::default())
    }
}

impl Workspace {
    /// A workspace with one pane holding one tab for `editor`.
    pub(crate) fn new(editor: Editor) -> Self {
        let pane = PaneId::new(0);
        let mut workspace = Self {
            documents: HashMap::new(),
            panes: Vec::new(),
            tree: Node::Leaf(pane),
            focused: pane,
            next_id: 1,
            confirm_close: None,
            confirm_bulk: None,
            closed: Vec::new(),
            recent: Vec::new(),
            confirm_overwrite: None,
            docs_version: 0,
            forgotten: Vec::new(),
            find_bar: 0,
            empty: (Buffer::default(), ViewState::default()),
            indent_prefs: IndentPrefs::default(),
            edit_events: Vec::new(),
        };
        let (view, buffer) = editor.into_parts();
        let document = workspace.add_document(buffer);
        let tab = workspace.new_tab(document, view);
        workspace.panes.push(Pane::with_tab(pane, tab));
        workspace
    }

    fn take_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    area.contains(ratatui::layout::Position::new(column, row))
}
