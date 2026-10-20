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
        /// How much of the split the first gets, in thousandths: a half to start with,
        /// moved by dragging the line between the two.
        ratio: u16,
    },
}

/// The ratio of a new split.
const HALF: u16 = 500;
/// The least and the most of a split the first of two may have, in thousandths.
const LEAST: u16 = 100;
const MOST: u16 = 900;

/// A line between two panes: which split it is (the way down from the root, `false` for
/// the first), which way it runs and the area the split fills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Divider {
    pub(super) path: Vec<bool>,
    pub(crate) axis: Axis,
    pub(crate) area: Rect,
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
                    ratio: HALF,
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

    /// The line between two panes at the cell (`x`, `y`) when the tree fills `area`: the last
    /// column of the first pane and the first of the second (or the rows), so that the border
    /// the two draw is all there is to grab. The deepest one wins.
    pub(super) fn divider_at(&self, area: Rect, x: u16, y: u16) -> Option<Divider> {
        self.find_divider(area, x, y, &mut Vec::new())
    }

    fn find_divider(&self, area: Rect, x: u16, y: u16, path: &mut Vec<bool>) -> Option<Divider> {
        let Self::Split {
            axis,
            first,
            second,
            ratio,
        } = self
        else {
            return None;
        };
        let (a, b) = halves(area, *axis, *ratio);
        for (child, part, step) in [(first, a, false), (second, b, true)] {
            path.push(step);
            let found = child.find_divider(part, x, y, path);
            path.pop();
            if found.is_some() {
                return found;
            }
        }
        let on = match axis {
            Axis::Horizontal => {
                (x + 1 == b.x || x == b.x) && y >= area.y && y < area.y + area.height
            }
            Axis::Vertical => (y + 1 == b.y || y == b.y) && x >= area.x && x < area.x + area.width,
        };
        on.then(|| Divider {
            path: path.clone(),
            axis: *axis,
            area,
        })
    }

    /// Gives the first of the split at `path` `ratio` thousandths of it, within what a
    /// pane may have.
    pub(super) fn set_ratio(&mut self, path: &[bool], new: u16) {
        match (self, path.split_first()) {
            (Self::Split { ratio, .. }, None) => *ratio = new.clamp(LEAST, MOST),
            (Self::Split { first, second, .. }, Some((&step, rest))) => match step {
                false => first.set_ratio(rest, new),
                true => second.set_ratio(rest, new),
            },
            (Self::Leaf(_), _) => {}
        }
    }

    /// The ratio of the split at `path`.
    pub(super) fn ratio(&self, path: &[bool]) -> Option<u16> {
        match (self, path.split_first()) {
            (Self::Split { ratio, .. }, None) => Some(*ratio),
            (Self::Split { first, second, .. }, Some((&step, rest))) => match step {
                false => first.ratio(rest),
                true => second.ratio(rest),
            },
            (Self::Leaf(_), _) => None,
        }
    }

    /// The way down to the split that has `pane` for a direct child, `None` for a single pane.
    pub(super) fn path_of_parent(&self, pane: PaneId) -> Option<Vec<bool>> {
        let Self::Split { first, second, .. } = self else {
            return None;
        };
        if **first == Self::Leaf(pane) || **second == Self::Leaf(pane) {
            return Some(Vec::new());
        }
        for (child, step) in [(first, false), (second, true)] {
            if let Some(mut path) = child.path_of_parent(pane) {
                path.insert(0, step);
                return Some(path);
            }
        }
        None
    }

    /// Whether `pane` is in the first part of the split at `path`.
    pub(super) fn in_first_of(&self, path: &[bool], pane: PaneId) -> Option<bool> {
        match (self, path.split_first()) {
            (Self::Split { first, .. }, None) => Some(first.leaves().contains(&pane)),
            (Self::Split { first, second, .. }, Some((&step, rest))) => match step {
                false => first.in_first_of(rest, pane),
                true => second.in_first_of(rest, pane),
            },
            (Self::Leaf(_), _) => None,
        }
    }

    /// The area of every pane when the tree fills `area`. Splits are in half to
    /// start with, then as far as they were moved; the second gets the odd cell.
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
                ratio,
            } => {
                let (a, b) = halves(area, *axis, *ratio);
                first.place(a, out);
                second.place(b, out);
            }
        }
    }
}

fn halves(area: Rect, axis: Axis, ratio: u16) -> (Rect, Rect) {
    let share = |whole: u16| (u32::from(whole) * u32::from(ratio) / 1000) as u16;
    match axis {
        Axis::Horizontal => {
            let left = share(area.width);
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
            let top = share(area.height);
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
