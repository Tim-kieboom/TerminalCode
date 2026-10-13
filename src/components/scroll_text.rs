//! Where a box of text that is longer than its rows is scrolled to, and where
//! on the screen it was drawn (so that the wheel over it can scroll it).
//! The numbers are `Cell`s because a box is drawn from `&self`.

use std::cell::Cell;

use ratatui::layout::Rect;

#[derive(Debug, Default)]
pub(crate) struct TextScroll {
    /// The first line shown.
    offset: Cell<usize>,
    /// The largest offset, as of the last draw.
    max: Cell<usize>,
    /// How many lines a page is, as of the last draw.
    page: Cell<usize>,
    /// Where the box was drawn; empty when it was not.
    area: Cell<Rect>,
}

impl TextScroll {
    /// Works out what a draw of `total` lines in `rows` rows shows: gives the
    /// first line, never past the last page.
    pub(crate) fn fit(&self, total: usize, rows: usize) -> usize {
        self.max.set(total.saturating_sub(rows));
        self.page.set(rows.max(1));
        let offset = self.offset.get().min(self.max.get());
        self.offset.set(offset);
        offset
    }

    /// Moves by `lines`, stopping at the ends.
    pub(crate) fn lines(&self, lines: isize) {
        let moved = self.offset.get().saturating_add_signed(lines);
        self.offset.set(moved.min(self.max.get()));
    }

    /// Moves by `pages`, a page being as many lines as the box shows.
    pub(crate) fn pages(&self, pages: isize) {
        let page = isize::try_from(self.page.get()).unwrap_or(isize::MAX);
        self.lines(pages.saturating_mul(page));
    }

    /// Back to the top.
    pub(crate) fn reset(&self) {
        self.offset.set(0);
    }

    /// Whether the box was drawn.
    pub(crate) fn is_drawn(&self) -> bool {
        let area = self.area.get();
        area.width > 0 && area.height > 0
    }

    /// Whether the last draw left lines out.
    pub(crate) fn is_scrollable(&self) -> bool {
        self.max.get() > 0
    }

    /// Notes where the box was drawn.
    pub(crate) fn drawn_at(&self, area: Rect) {
        self.area.set(area);
    }

    /// Notes that the box was not drawn.
    pub(crate) fn not_drawn(&self) {
        self.area.set(Rect::default());
    }

    /// Whether the screen cell is in the box as it was drawn.
    pub(crate) fn contains(&self, column: u16, row: u16) -> bool {
        let area = self.area.get();
        area.width > 0
            && area.height > 0
            && column >= area.x
            && column < area.right()
            && row >= area.y
            && row < area.bottom()
    }

    /// ` 3-16/40 `: which lines show, when not all of them do.
    pub(crate) fn label(&self, total: usize, rows: usize) -> Option<String> {
        (total > rows).then(|| {
            let first = self.offset.get() + 1;
            format!(" {first}-{}/{total} ", (first + rows - 1).min(total))
        })
    }
}
