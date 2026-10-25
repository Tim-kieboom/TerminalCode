//! The box both the code-action picker and the definition picker are: a list of
//! entries in the upper middle of the screen, one row each, one selected, with
//! the keys' arithmetic (a step wraps round the ends, a page stops at them) and
//! the mouse's (a click on a row selects it). What a row looks like is up to the
//! caller; this draws the box and the rows it hands back.

use std::cell::Cell;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::components::list_cursor::revealed;
use crate::ui::theme::Theme;

/// How many rows show at once; the rest scrolls.
const MOST_ROWS: usize = 12;
/// The box is at least this wide when the screen allows.
const NARROWEST: u16 = 40;
/// The box is too small to show anything below this.
const SMALLEST: u16 = 8;

#[derive(Debug)]
pub(crate) struct PickList<T> {
    entries: Vec<T>,
    selected: usize,
    /// The first row shown.
    scroll: Cell<usize>,
    /// Where it was last drawn, for clicks; empty until the first draw.
    area: Cell<Rect>,
}

impl<T> PickList<T> {
    pub(crate) fn new(entries: Vec<T>) -> Self {
        Self {
            entries,
            selected: 0,
            scroll: Cell::new(0),
            area: Cell::new(Rect::default()),
        }
    }

    /// The same list with row `selected` selected (the last row when it is past the end).
    pub(crate) fn selecting(mut self, selected: usize) -> Self {
        self.selected = selected.min(self.entries.len().saturating_sub(1));
        self
    }

    pub(crate) fn entries(&self) -> &[T] {
        &self.entries
    }

    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    pub(crate) fn selected_entry(&self) -> Option<&T> {
        self.entries.get(self.selected)
    }

    /// Moves the selection by `rows`, round the ends for a step of one.
    pub(crate) fn step(&mut self, rows: isize) {
        let count = isize::try_from(self.entries.len()).unwrap_or(isize::MAX);
        if count == 0 {
            return;
        }
        let at = isize::try_from(self.selected).unwrap_or(0);
        let moved = at + rows;
        // A step of one goes round; a page stops at the ends.
        self.selected = match rows.abs() {
            1 => moved.rem_euclid(count),
            _ => moved.clamp(0, count - 1),
        } as usize;
    }

    /// Moves by `pages` of the rows that show at once.
    pub(crate) fn page(&mut self, pages: isize) {
        self.step(pages * isize::try_from(MOST_ROWS).unwrap_or(1));
    }

    /// Selects the entry at `index` (from 0), if there is one. Whether there was.
    pub(crate) fn select_index(&mut self, index: usize) -> bool {
        if index >= self.entries.len() {
            return false;
        }
        self.selected = index;
        true
    }

    /// Selects the row at the screen cell, if it is one. Whether it was.
    pub(crate) fn select_at(&mut self, column: u16, row: u16) -> bool {
        let area = self.area.get();
        let inside =
            column > area.x && column + 1 < area.right() && row > area.y && row + 1 < area.bottom();
        if !inside {
            return false;
        }
        let index = self.scroll.get() + usize::from(row - area.y - 1);
        self.select_index(index)
    }

    /// Whether the screen cell is inside the box as it was drawn.
    pub(crate) fn contains(&self, column: u16, row: u16) -> bool {
        let area = self.area.get();
        area.width > 0
            && column >= area.x
            && column < area.right()
            && row >= area.y
            && row < area.bottom()
    }

    /// Draws the box, at most `widest` columns wide, called ` title (count) `,
    /// and a row for each entry that shows. `row` makes the line of an entry
    /// from it, its index, and the columns it has; the selected line is
    /// highlighted here.
    pub(crate) fn draw(
        &self,
        frame: &mut Frame,
        theme: &Theme,
        title: &str,
        widest: u16,
        row: impl Fn(&T, usize, usize) -> Line<'static>,
    ) {
        let screen = frame.area();
        let width = widest
            .min(screen.width.saturating_sub(4))
            .max(NARROWEST.min(screen.width));
        let rows = self.entries.len().min(MOST_ROWS);
        let height = u16::try_from(rows).unwrap_or(u16::MAX) + 2;
        if width < SMALLEST || screen.height < height {
            self.area.set(Rect::default());
            return;
        }
        let area = Rect {
            x: screen.x + (screen.width - width) / 2,
            y: screen.y + (screen.height / 5).min(screen.height - height),
            width,
            height,
        };
        self.area.set(area);

        // Keeps the selected row among the rows shown.
        let first = revealed(self.selected, self.scroll.get(), rows)
            .min(self.entries.len().saturating_sub(rows));
        self.scroll.set(first);

        let title = format!(" {title} ({}) ", self.entries.len());
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border.focused"))
            .title(Line::styled(title, theme.style("pane.title")));
        let inner = block.inner(area);
        crate::ui::backdrop::clear(frame, theme, area);
        frame.render_widget(block, area);

        let columns = usize::from(inner.width);
        let lines: Vec<Line> = self
            .entries
            .iter()
            .enumerate()
            .skip(first)
            .take(rows)
            .map(|(index, entry)| {
                let line = row(entry, index, columns);
                match index == self.selected {
                    true => line.style(theme.style("completion.selected")),
                    false => line,
                }
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

#[cfg(test)]
mod tests;
