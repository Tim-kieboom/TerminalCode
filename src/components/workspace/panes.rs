//! Panes: splitting, moving focus between them, and where each is on screen.

use super::tree::Divider;
use super::{FocusDirection, Pane, Tab, Workspace, contains};
use crate::ids::PaneId;
use crate::ui::layout::Axis;
use ratatui::layout::Rect;

impl Workspace {
    /// Reserves (or gives back) rows at the bottom of the focused pane for the
    /// find bar: 0 for none, 1 for the find row, 2 with the replace row.
    pub(crate) fn set_find_bar_rows(&mut self, rows: u16) {
        self.find_bar = rows;
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

    /// The screen area of the focused pane, as of the last layout pass.
    pub(crate) fn focused_area(&self) -> Rect {
        self.focused_pane().area
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
        let id = PaneId::new(self.take_id());
        self.tree.split(self.focused, axis, id);
        self.panes.push(Pane::with_tab(id, tab));
        self.focused = id;
    }

    /// The line between two panes at (`x`, `y`) in the editor area `area`, if there is one.
    pub(crate) fn divider_at(&self, area: Rect, x: u16, y: u16) -> Option<Divider> {
        self.tree.divider_at(area, x, y)
    }

    /// Moves `divider` to where the pointer is: the first pane ends where it is.
    pub(crate) fn drag_divider(&mut self, divider: &Divider, x: u16, y: u16) {
        let area = divider.area;
        let (inside, whole) = match divider.axis {
            Axis::Horizontal => (x.saturating_sub(area.x), area.width),
            Axis::Vertical => (y.saturating_sub(area.y), area.height),
        };
        if whole == 0 {
            return;
        }
        let ratio = (u32::from(inside) * 1000 / u32::from(whole)).min(1000) as u16;
        self.tree.set_ratio(&divider.path, ratio);
    }

    /// Makes the focused pane bigger (`step` thousandths of its split, or smaller when
    /// negative) at the cost of the pane next to it. Returns whether there was a split.
    pub(crate) fn resize_focused(&mut self, step: i16) -> bool {
        let Some(path) = self.tree.path_of_parent(self.focused) else {
            return false;
        };
        let (Some(ratio), Some(in_first)) = (
            self.tree.ratio(&path),
            self.tree.in_first_of(&path, self.focused),
        ) else {
            return false;
        };
        let wanted = i32::from(ratio)
            + if in_first {
                i32::from(step)
            } else {
                -i32::from(step)
            };
        self.tree.set_ratio(&path, wanted.clamp(0, 1000) as u16);
        true
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

    /// The pane nearest to the area `from` in `direction`, with how near it is
    /// (smaller is nearer; see [`neighbor_score`]), going by the screen areas of
    /// the last layout pass. The focused pane is left out when `from` is its
    /// own area.
    pub(crate) fn nearest_pane(
        &self,
        from: Rect,
        direction: FocusDirection,
    ) -> Option<(PaneId, (u32, u32))> {
        self.panes
            .iter()
            .filter(|pane| pane.area != from)
            .filter_map(|pane| Some((pane.id, neighbor_score(from, pane.area, direction)?)))
            .min_by_key(|(_, score)| *score)
    }

    /// Moves focus to the nearest pane in `direction`, if there is one.
    #[cfg(test)]
    pub(crate) fn focus_direction(&mut self, direction: FocusDirection) {
        let from = self.focused_pane().area;
        if let Some((id, _)) = self.nearest_pane(from, direction) {
            self.focus_pane(id);
        }
    }

    /// Gives the focus to the pane `id`, if there is one.
    pub(crate) fn focus_pane(&mut self, id: PaneId) {
        self.reset_confirmations();
        if self.panes.iter().any(|pane| pane.id == id) {
            self.focused = id;
        }
    }

    /// Drops the pane at `index` after its last tab was closed. The only pane
    /// is kept, empty.
    pub(super) fn remove_pane(&mut self, index: usize) {
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

    pub(super) fn pane_at(&self, column: u16, row: u16) -> Option<usize> {
        self.panes
            .iter()
            .position(|pane| contains(pane.area, column, row))
    }

    pub(super) fn focused_index(&self) -> usize {
        self.panes
            .iter()
            .position(|pane| pane.id == self.focused)
            .unwrap_or(0)
    }

    pub(super) fn focused_pane(&self) -> &Pane {
        &self.panes[self.focused_index()]
    }

    pub(super) fn focused_pane_mut(&mut self) -> &mut Pane {
        let index = self.focused_index();
        &mut self.panes[index]
    }
}

impl Pane {
    /// The tab a click on an overflow marker goes to: the one just before the
    /// first tab in view, or just after the last.
    pub(super) fn hidden_tab_at(&self, column: u16, row: u16) -> Option<usize> {
        let visible = |rect: &Rect| rect.width > 0;
        let (before, after) = self.overflow;
        if self.hidden.0 > 0 && contains(before, column, row) {
            return self.tab_rects.iter().position(visible)?.checked_sub(1);
        }
        if self.hidden.1 > 0 && contains(after, column, row) {
            let next = self.tab_rects.iter().rposition(visible)? + 1;
            return (next < self.tabs.len()).then_some(next);
        }
        None
    }

    /// The tab drawn at a screen cell, going by the last layout pass. Tabs can
    /// have been closed since, so a rectangle left over from the last frame
    /// does not count if its tab is gone.
    pub(super) fn tab_at(&self, column: u16, row: u16) -> Option<usize> {
        let tab = self
            .tab_rects
            .iter()
            .position(|rect| contains(*rect, column, row))?;

        (tab < self.tabs.len()).then_some(tab)
    }

    /// A pane that holds just `tab`.
    pub(super) fn with_tab(id: PaneId, tab: Tab) -> Self {
        Self {
            id,
            tabs: vec![tab],
            active: 0,
            area: Rect::default(),
            tab_bar: Rect::default(),
            tab_rects: Vec::new(),
            overflow: (Rect::default(), Rect::default()),
            hidden: (0, 0),
            editor_area: Rect::default(),
            find_bar: Rect::default(),
        }
    }
}

/// How good a neighbor `other` is for moving from `from` in `direction`:
/// smaller is nearer, and `None` if it is not in that direction or does not
/// overlap `from` on the other axis.
pub(crate) fn neighbor_score(
    from: Rect,
    other: Rect,
    direction: FocusDirection,
) -> Option<(u32, u32)> {
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
