//! The selection and the first visible row of a scrolling list, with the
//! arithmetic every list needs: moving by a row or a page, keeping the selection
//! in view, scrolling without moving it, and clicking on a row.
//!
//! [`ListCursor`] holds both numbers. [`stepped`] and [`revealed`] are the same
//! arithmetic on bare numbers, for a list (the explorer) whose own code reads
//! its selection and scroll directly.

/// A move of the selection that stops at the ends of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    Up,
    Down,
    PageUp,
    PageDown,
    First,
    Last,
}

/// A selection and the first row shown, of one list.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ListCursor {
    selected: usize,
    scroll: usize,
}

impl ListCursor {
    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    /// The first row shown.
    pub(crate) fn scroll(&self) -> usize {
        self.scroll
    }

    /// Selects `index`, or the last row when it is past the end.
    pub(crate) fn select(&mut self, index: usize, count: usize) {
        self.selected = index.min(count.saturating_sub(1));
    }

    /// Moves the selection by `step` among `count` rows; a page is `page` rows.
    pub(crate) fn step(&mut self, step: Step, count: usize, page: usize) {
        self.selected = stepped(self.selected, step, count, page);
    }

    /// Moves the selection by `rows` (negative is up), stopping at the ends.
    pub(crate) fn shift(&mut self, rows: isize, count: usize) {
        let last = count.saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(rows).min(last);
    }

    /// Selects the row at `offset` rows below the first one shown, if it is one.
    /// Whether it was.
    pub(crate) fn select_row(&mut self, offset: usize, count: usize) -> bool {
        let index = self.scroll + offset;
        if index >= count {
            return false;
        }
        self.selected = index;
        true
    }

    /// Scrolls by `rows` (negative is up) without moving the selection, never
    /// past the point where the last row is at the bottom of `height` lines.
    pub(crate) fn scroll_by(&mut self, rows: isize, count: usize, height: usize) {
        let top = self.scroll.saturating_add_signed(rows);
        self.scroll = top.min(most_scroll(count, height));
    }

    /// Brings the selection and the scroll back inside `count` rows in `height` lines.
    pub(crate) fn clamp(&mut self, count: usize, height: usize) {
        self.selected = self.selected.min(count.saturating_sub(1));
        self.scroll = self.scroll.min(most_scroll(count, height));
    }

    /// Scrolls just far enough that the selection shows in `height` lines.
    pub(crate) fn reveal(&mut self, height: usize) {
        self.scroll = revealed(self.selected, self.scroll, height);
    }

    /// Back to the first row.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

/// `selected` moved by `step` among `count` rows, stopping at the ends.
pub(crate) fn stepped(selected: usize, step: Step, count: usize, page: usize) -> usize {
    let last = count.saturating_sub(1);
    match step {
        Step::Up => selected.saturating_sub(1),
        Step::Down => (selected + 1).min(last),
        Step::PageUp => selected.saturating_sub(page),
        Step::PageDown => (selected + page).min(last),
        Step::First => 0,
        Step::Last => last,
    }
}

/// The scroll nearest to `scroll` that shows `selected` in `height` lines. With
/// no lines the scroll stays.
pub(crate) fn revealed(selected: usize, scroll: usize, height: usize) -> usize {
    if height == 0 {
        return scroll;
    }
    if selected < scroll {
        selected
    } else if selected >= scroll + height {
        selected + 1 - height
    } else {
        scroll
    }
}

fn most_scroll(count: usize, height: usize) -> usize {
    count.saturating_sub(height.max(1))
}
