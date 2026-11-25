//! The tabs and panes as a session (see `persist::session`): what is kept of them, and
//! putting them back.

use std::path::{Path, PathBuf};

use super::tree::Node;
use super::{Pane, Tab, Workspace};
use crate::buffer::{Buffer, Position};
use crate::components::editor::{Scroll, ViewState};
use crate::files::paths::canonical;
use crate::ids::PaneId;
use crate::persist::session::{Direction, Layout, PaneSession, Session, TabSession};
use crate::ui::layout::Axis;

impl Workspace {
    /// The tabs and panes to keep for the project folder `project`. Only tabs of files
    /// count: a tab that has no file (unsaved text, a diff) is left out, and a pane with
    /// no such tab goes with it. `None` when no tab is left.
    pub(crate) fn session(&self, project: &Path) -> Option<Session> {
        let layout = self.layout_of(&self.tree)?;
        Some(Session::new(project, layout))
    }

    /// Brings a session back: panes, tabs, which tab is in front, pinned tabs, each cursor
    /// and scroll. It only does so while nothing was opened yet (one pane with one empty,
    /// unmodified tab), so it never takes tabs away from a file named on the command line.
    /// Returns the files of the session that could not be opened.
    pub(crate) fn restore_session(&mut self, session: &Session) -> Vec<PathBuf> {
        if !self.is_untouched() {
            return Vec::new();
        }
        let before = std::mem::take(&mut self.panes);
        let mut skipped = Vec::new();
        let mut focused = None;
        let Some(tree) = self.build(&session.layout, &mut skipped, &mut focused) else {
            self.panes = before;
            return skipped;
        };
        // The empty tab of the start goes: its document has no other view.
        for pane in before {
            for tab in pane.tabs {
                if let Some(removed) = self.documents.remove(&tab.document) {
                    self.forgotten.push(removed.syntax.key());
                }
            }
        }
        self.docs_version += 1;
        self.tree = tree;
        self.focused = focused.unwrap_or_else(|| self.tree.leaves()[0]);
        skipped
    }

    /// Whether the workspace is as it is at the start: one pane with one tab, which shows
    /// an empty text that has no file and was not edited.
    fn is_untouched(&self) -> bool {
        let [pane] = self.panes.as_slice() else {
            return false;
        };
        let [tab] = pane.tabs.as_slice() else {
            return false;
        };
        let Some(doc) = self.documents.get(&tab.document) else {
            return false;
        };
        tab.diff.is_none()
            && doc.buffer.path().is_none()
            && !doc.buffer.is_dirty()
            && doc.buffer.len_bytes() == 0
    }

    fn layout_of(&self, node: &Node) -> Option<Layout> {
        match node {
            Node::Leaf(id) => self.pane_session(*id).map(Layout::Pane),
            Node::Split {
                axis,
                first,
                second,
                ratio,
            } => match (self.layout_of(first), self.layout_of(second)) {
                (Some(first), Some(second)) => Some(Layout::Split {
                    direction: direction_of(*axis),
                    ratio: *ratio,
                    first: Box::new(first),
                    second: Box::new(second),
                }),
                (only, None) | (None, only) => only,
            },
        }
    }

    fn pane_session(&self, id: PaneId) -> Option<PaneSession> {
        let pane = self.panes.iter().find(|pane| pane.id == id)?;
        let mut tabs = Vec::new();
        let mut active = 0;
        for (index, tab) in pane.tabs.iter().enumerate() {
            let Some(saved) = self.tab_session(tab) else {
                continue;
            };
            if index <= pane.active {
                active = tabs.len();
            }
            tabs.push(saved);
        }
        if tabs.is_empty() {
            return None;
        }
        Some(PaneSession {
            tabs,
            active,
            focused: id == self.focused,
        })
    }

    fn tab_session(&self, tab: &Tab) -> Option<TabSession> {
        if tab.diff.is_some() {
            return None;
        }
        let path = self.documents.get(&tab.document)?.buffer.path()?;
        let head = tab.view.selections().primary().head();
        let scroll = tab.view.scroll();
        Some(TabSession {
            path: canonical(path),
            pinned: tab.pinned,
            line: head.line,
            column: head.column,
            top: scroll.top,
            left: scroll.left,
        })
    }

    /// Builds the panes of `layout` into `self.panes` and returns how they are split;
    /// `None` when none of its files could be opened.
    fn build(
        &mut self,
        layout: &Layout,
        skipped: &mut Vec<PathBuf>,
        focused: &mut Option<PaneId>,
    ) -> Option<Node> {
        match layout {
            Layout::Pane(saved) => self.build_pane(saved, skipped, focused),
            Layout::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let first = self.build(first, skipped, focused);
                let second = self.build(second, skipped, focused);
                match (first, second) {
                    (Some(first), Some(second)) => {
                        Some(Node::joined(axis_of(*direction), first, second, *ratio))
                    }
                    (only, None) | (None, only) => only,
                }
            }
        }
    }

    fn build_pane(
        &mut self,
        saved: &PaneSession,
        skipped: &mut Vec<PathBuf>,
        focused: &mut Option<PaneId>,
    ) -> Option<Node> {
        let mut tabs = Vec::with_capacity(saved.tabs.len());
        let mut active = 0;
        for (index, saved_tab) in saved.tabs.iter().enumerate() {
            let Some(tab) = self.build_tab(saved_tab) else {
                skipped.push(saved_tab.path.clone());
                continue;
            };
            if index <= saved.active {
                active = tabs.len();
            }
            tabs.push(tab);
        }
        let mut tabs = tabs.into_iter();
        let first = tabs.next()?;
        let id = PaneId::new(self.take_id());
        let mut pane = Pane::with_tab(id, first);
        pane.tabs.extend(tabs);
        pane.active = active;
        self.panes.push(pane);
        if saved.focused {
            *focused = Some(id);
        }
        Some(Node::Leaf(id))
    }

    /// A tab for the file of `saved`, sharing the document of a tab that shows it already.
    fn build_tab(&mut self, saved: &TabSession) -> Option<Tab> {
        let document = match self.document_of_path(&saved.path) {
            Some(document) => document,
            None => {
                let buffer = Buffer::open(&saved.path).ok()?;
                self.add_document(buffer)
            }
        };
        let cursor = self
            .documents
            .get(&document)?
            .buffer
            .clamp_position(Position::new(saved.line, saved.column));
        let mut view = ViewState::default();
        view.restore(
            cursor,
            Scroll {
                top: saved.top,
                left: saved.left,
            },
        );
        let mut tab = self.new_tab(document, view);
        tab.pinned = saved.pinned;
        Some(tab)
    }
}

fn direction_of(axis: Axis) -> Direction {
    match axis {
        Axis::Horizontal => Direction::Horizontal,
        Axis::Vertical => Direction::Vertical,
    }
}

fn axis_of(direction: Direction) -> Axis {
    match direction {
        Direction::Horizontal => Axis::Horizontal,
        Direction::Vertical => Axis::Vertical,
    }
}
