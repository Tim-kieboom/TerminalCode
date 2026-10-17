//! Tabs: closing, pinning, moving between them and the clicks on the strip.

use super::{CloseResult, Tab, Workspace};
use crate::components::editor::{ViewState, display_name};
use crate::ids::{DocumentId, ViewId};
#[cfg(test)]
use ratatui::layout::Rect;

impl Workspace {
    /// Whether the focused pane has a tab. After the last tab is closed the
    /// editor area is empty until a new file is opened.
    pub(crate) fn has_tabs(&self) -> bool {
        !self.focused_pane().tabs.is_empty()
    }

    /// How many tabs there are in all panes.
    #[cfg(test)]
    pub(crate) fn tab_count_for_test(&self) -> usize {
        self.panes.iter().map(|pane| pane.tabs.len()).sum()
    }

    /// Screen areas of the focused pane's tabs as of the last layout pass.
    #[cfg(test)]
    pub(crate) fn tab_areas(&self) -> Vec<Rect> {
        self.focused_pane().tab_rects.clone()
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

    pub(super) fn find_tab_of(&self, document: DocumentId) -> Option<(usize, usize)> {
        self.panes.iter().enumerate().find_map(|(pane, p)| {
            let tab = p.tabs.iter().position(|tab| tab.document == document)?;
            Some((pane, tab))
        })
    }

    /// Forgets the pending "close again" and "save again" confirmations; any
    /// other action in between cancels them.
    pub(super) fn reset_confirmations(&mut self) {
        self.confirm_close = None;
        self.confirm_bulk = None;
        self.confirm_overwrite = None;
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

    /// Pins the active tab of the focused pane, or lets it go. A pinned tab moves
    /// to the end of the pinned ones at the left of the strip; one let go to just
    /// after them. Whether the tab is pinned now.
    pub(crate) fn toggle_pin_active(&mut self) -> bool {
        self.reset_confirmations();
        let pane = self.focused_pane_mut();
        if pane.active >= pane.tabs.len() {
            return false;
        }
        let mut tab = pane.tabs.remove(pane.active);
        tab.pinned = !tab.pinned;
        let pinned = tab.pinned;
        let first_loose = pane.tabs.iter().filter(|other| other.pinned).count();
        pane.tabs.insert(first_loose, tab);
        pane.active = first_loose;
        pinned
    }

    /// The names of the tabs of the focused pane, left to right.
    #[cfg(test)]
    pub(crate) fn tab_names_for_test(&self) -> Vec<String> {
        self.focused_pane()
            .tabs
            .iter()
            .map(|tab| display_name(&self.documents[&tab.document].buffer))
            .collect()
    }

    /// Whether the active tab of the focused pane is pinned.
    pub(crate) fn active_tab_is_pinned(&self) -> bool {
        self.active_tab().is_some_and(|tab| tab.pinned)
    }

    /// A right button press on a tab strip: brings that tab to the front, in its
    /// pane, which gets the focus (the menu works on the tab in front). Returns
    /// whether it was on a tab.
    pub(crate) fn tab_context_press(&mut self, column: u16, row: u16) -> bool {
        let Some(pane) = self.pane_at(column, row) else {
            return false;
        };
        let Some(tab) = self.panes[pane].tab_at(column, row) else {
            return false;
        };
        // The same tab again is nothing new, so a warning that waits for the same
        // close to be asked again still waits.
        let moved = self.focused != self.panes[pane].id || self.panes[pane].active != tab;
        if moved {
            self.reset_confirmations();
        }
        self.focused = self.panes[pane].id;
        self.panes[pane].active = tab;
        true
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
    pub(super) fn close_tab_at(&mut self, index: usize, tab_index: usize) -> CloseResult {
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

        self.close_remembering(index, tab_index);
        CloseResult::Closed
    }

    /// Removes a tab without asking. The document goes with its last view.
    pub(super) fn remove_tab(&mut self, index: usize, tab_index: usize) {
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
            if let Some(removed) = self.documents.remove(&document) {
                self.forgotten.push(removed.syntax.key());
            }
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

    /// Positions of every other tab showing `document`, except `skip`.
    pub(super) fn other_tabs(
        &self,
        document: DocumentId,
        skip: (usize, usize),
    ) -> Vec<(usize, usize)> {
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

    pub(super) fn new_tab(&mut self, document: DocumentId, mut view: ViewState) -> Tab {
        view.set_tab_width(self.indent_prefs.size);
        Tab {
            id: ViewId::new(self.take_id()),
            document,
            view,
            pinned: false,
        }
    }

    pub(super) fn views_of(&self, document: DocumentId) -> usize {
        self.panes
            .iter()
            .flat_map(|pane| &pane.tabs)
            .filter(|tab| tab.document == document)
            .count()
    }

    pub(super) fn step_tab(&mut self, delta: isize) {
        self.reset_confirmations();
        let pane = self.focused_pane_mut();
        let count = pane.tabs.len() as isize;
        if count == 0 {
            return;
        }
        pane.active = (pane.active as isize + delta).rem_euclid(count) as usize;
    }

    pub(super) fn active_tab(&self) -> Option<&Tab> {
        let pane = self.focused_pane();
        pane.tabs.get(pane.active)
    }
}
