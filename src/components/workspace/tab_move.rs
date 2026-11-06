//! Moving the tabs of a strip: a key or a drag moves the tab in front one place at a time,
//! and the wheel over the strip steps through the tabs. A pinned tab stays among the pinned
//! ones at the left and a loose tab among the loose: only pinning and unpinning cross the line.

use std::ops::Range;

use super::{Pane, Workspace, contains};

impl Pane {
    /// Where tab `from` may go: among the pinned tabs when it is pinned, else among the others.
    fn lane_of(&self, from: usize) -> Range<usize> {
        let pinned = self.tabs.iter().filter(|tab| tab.pinned).count();
        match self.tabs.get(from).is_some_and(|tab| tab.pinned) {
            true => 0..pinned,
            false => pinned..self.tabs.len(),
        }
    }

    /// Moves tab `from` to `to`, held to its lane, and makes it the tab in front. Returns
    /// whether it moved.
    fn move_tab(&mut self, from: usize, to: usize) -> bool {
        let lane = self.lane_of(from);
        if lane.is_empty() {
            return false;
        }
        let to = to.clamp(lane.start, lane.end - 1);
        if to == from {
            return false;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        self.active = to;
        // Where the tabs were drawn is not where they are now, until the next draw.
        self.tab_rects.clear();
        true
    }

    /// The tab the pointer is over in the strip, going by the last draw: the one under it,
    /// or the first or last in view when it is past an end.
    fn tab_for_drag(&self, column: u16) -> Option<usize> {
        let visible: Vec<usize> = (0..self.tabs.len())
            .filter(|index| {
                self.tab_rects
                    .get(*index)
                    .is_some_and(|rect| rect.width > 0)
            })
            .collect();
        let (first, last) = (*visible.first()?, *visible.last()?);
        if column < self.tab_rects[first].x {
            return Some(first);
        }
        if column >= self.tab_rects[last].right() {
            return Some(last);
        }
        visible.into_iter().find(|index| {
            self.tab_rects[*index].x <= column && column < self.tab_rects[*index].right()
        })
    }
}

impl Workspace {
    /// Moves the tab in front of the focused pane `delta` places (negative is left), held to
    /// its lane. Returns whether it moved.
    pub(crate) fn move_active_tab(&mut self, delta: isize) -> bool {
        let pane = self.focused_pane_mut();
        let Some(to) = pane.active.checked_add_signed(delta) else {
            return false;
        };
        let from = pane.active;
        pane.move_tab(from, to)
    }

    /// The pointer moved with the left button held on a tab: the tab follows it along the
    /// strip. Returns whether a tab is held (the drag is then not for the text).
    pub(crate) fn drag_tab(&mut self, column: u16) -> bool {
        let Some((pane_id, view)) = self.tab_drag else {
            return false;
        };
        let Some(index) = self.panes.iter().position(|pane| pane.id == pane_id) else {
            self.tab_drag = None;
            return false;
        };
        let pane = &mut self.panes[index];
        let Some(from) = pane.tabs.iter().position(|tab| tab.id == view) else {
            self.tab_drag = None;
            return false;
        };
        if let Some(to) = pane.tab_for_drag(column) {
            pane.move_tab(from, to);
        }
        true
    }

    /// The left button was let go: whatever tab was held is let go too.
    pub(crate) fn end_tab_drag(&mut self) {
        self.tab_drag = None;
    }

    /// The wheel over the strip of a pane: the next tab for a positive `delta`, the previous
    /// for a negative one, without going round. The strip shows the tab in front, so it
    /// scrolls with it. Returns whether the pointer was on a strip.
    pub(crate) fn scroll_tabs(&mut self, column: u16, row: u16, delta: isize) -> bool {
        let Some(index) = self.pane_at(column, row) else {
            return false;
        };
        let pane = &self.panes[index];
        if pane.tabs.is_empty() || !contains(pane.tab_bar, column, row) {
            return false;
        }
        let last = pane.tabs.len() - 1;
        let target = pane.active.saturating_add_signed(delta).min(last);
        let moved = target != pane.active || self.focused != pane.id;
        if moved {
            self.reset_confirmations();
        }
        self.focused = self.panes[index].id;
        self.panes[index].active = target;
        true
    }
}
