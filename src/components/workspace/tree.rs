use ratatui::layout::Rect;

use super::PaneId;
use crate::ui::layout::Axis;

/// How panes are split up: a binary tree whose leaves are panes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Node {
    Leaf(PaneId),
    Split {
        axis: Axis,
        first: Box<Node>,
        second: Box<Node>,
    },
}

impl Node {
    /// Replaces the leaf `target` with a split holding it and `new`, `new`
    /// second (right of it, or below it). Returns whether `target` was found.
    pub(super) fn split(&mut self, target: PaneId, axis: Axis, new: PaneId) -> bool {
        match self {
            Self::Leaf(id) if *id == target => {
                *self = Self::Split {
                    axis,
                    first: Box::new(Self::Leaf(target)),
                    second: Box::new(Self::Leaf(new)),
                };
                true
            }
            Self::Leaf(_) => false,
            Self::Split { first, second, .. } => {
                first.split(target, axis, new) || second.split(target, axis, new)
            }
        }
    }

    /// Removes the leaf `pane`; its sibling takes the space. Returns whether
    /// anything was removed (the only leaf of a tree cannot be removed).
    pub(super) fn remove(&mut self, pane: PaneId) -> bool {
        let Self::Split { first, second, .. } = self else {
            return false;
        };
        if **first == Self::Leaf(pane) {
            *self = std::mem::replace(&mut **second, Self::Leaf(pane));
            return true;
        }
        if **second == Self::Leaf(pane) {
            *self = std::mem::replace(&mut **first, Self::Leaf(pane));
            return true;
        }
        first.remove(pane) || second.remove(pane)
    }

    /// Pane ids in reading order (left to right, top to bottom).
    pub(super) fn leaves(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        self.collect_leaves(&mut out);
        out
    }

    fn collect_leaves(&self, out: &mut Vec<PaneId>) {
        match self {
            Self::Leaf(id) => out.push(*id),
            Self::Split { first, second, .. } => {
                first.collect_leaves(out);
                second.collect_leaves(out);
            }
        }
    }

    /// The area of every pane when the tree fills `area`. Splits are in half;
    /// the second half gets the odd cell.
    pub(super) fn layout(&self, area: Rect) -> Vec<(PaneId, Rect)> {
        let mut out = Vec::new();
        self.place(area, &mut out);
        out
    }

    fn place(&self, area: Rect, out: &mut Vec<(PaneId, Rect)>) {
        match self {
            Self::Leaf(id) => out.push((*id, area)),
            Self::Split {
                axis,
                first,
                second,
            } => {
                let (a, b) = halves(area, *axis);
                first.place(a, out);
                second.place(b, out);
            }
        }
    }
}

fn halves(area: Rect, axis: Axis) -> (Rect, Rect) {
    match axis {
        Axis::Horizontal => {
            let left = area.width / 2;
            (
                Rect {
                    width: left,
                    ..area
                },
                Rect {
                    x: area.x + left,
                    width: area.width - left,
                    ..area
                },
            )
        }
        Axis::Vertical => {
            let top = area.height / 2;
            (
                Rect {
                    height: top,
                    ..area
                },
                Rect {
                    y: area.y + top,
                    height: area.height - top,
                    ..area
                },
            )
        }
    }
}
