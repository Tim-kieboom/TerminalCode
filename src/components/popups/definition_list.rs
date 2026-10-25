//! The places to choose from when a server names several definitions (a trait
//! method has one for each implementation): a box in the upper middle of the
//! screen, a row for each place with its file, its line number and the text of
//! that line. Up and down move, Enter goes, Esc closes; a click on a row goes.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::pick_list::PickList;
use super::plain_key;
use crate::components::text::fit;
pub(crate) use crate::components::text::short_path;
use crate::lsp::{Location, ServerKey};
use crate::ui::theme::Theme;

/// The box is at most this wide.
const WIDEST: u16 = 110;

/// What a key press asks the list to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Previous,
    Next,
    PageUp,
    PageDown,
    Go,
    Cancel,
}

impl Command {
    pub(crate) fn from_key(key: KeyEvent) -> Option<Self> {
        Some(match plain_key(key)? {
            KeyCode::Up => Self::Previous,
            KeyCode::Down => Self::Next,
            KeyCode::PageUp => Self::PageUp,
            KeyCode::PageDown => Self::PageDown,
            KeyCode::Enter => Self::Go,
            KeyCode::Esc => Self::Cancel,
            _ => return None,
        })
    }
}

/// One place: where to show it, and what is on its line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    /// `src/lib.rs:12`: the file as short as it can be, and the line from 1.
    pub(crate) place: String,
    /// The text of the line, without its indent.
    pub(crate) text: String,
    pub(crate) location: Location,
}

#[derive(Debug)]
pub(crate) struct DefinitionList {
    list: PickList<Entry>,
    /// The server that named the places: the files outside the project stay with it.
    key: ServerKey,
}

impl DefinitionList {
    pub(crate) fn new(entries: Vec<Entry>, key: ServerKey) -> Self {
        Self {
            list: PickList::new(entries),
            key,
        }
    }

    #[cfg(test)]
    pub(crate) fn entries(&self) -> &[Entry] {
        self.list.entries()
    }

    pub(crate) fn key(&self) -> &ServerKey {
        &self.key
    }

    #[cfg(test)]
    pub(crate) fn selected(&self) -> usize {
        self.list.selected()
    }

    pub(crate) fn selected_location(&self) -> Option<&Location> {
        self.list.selected_entry().map(|entry| &entry.location)
    }

    /// Moves the selection by `rows`, round the ends for a step of one.
    pub(crate) fn step(&mut self, rows: isize) {
        self.list.step(rows);
    }

    /// Moves by a page.
    pub(crate) fn page(&mut self, pages: isize) {
        self.list.page(pages);
    }

    /// Selects the row at the screen cell, if it is one. Whether it was.
    pub(crate) fn select_at(&mut self, column: u16, row: u16) -> bool {
        self.list.select_at(column, row)
    }

    /// Whether the screen cell is inside the box as it was drawn.
    pub(crate) fn contains(&self, column: u16, row: u16) -> bool {
        self.list.contains(column, row)
    }

    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        // The places line up in a column as wide as the widest, up to half the row.
        let widest_place = self
            .list
            .entries()
            .iter()
            .map(|entry| entry.place.width())
            .max()
            .unwrap_or(0);
        self.list
            .draw(frame, theme, "Definitions", WIDEST, |entry, _, width| {
                row(entry, width, widest_place, theme)
            });
    }
}

/// The row of one place: the place in a column, then the text of its line.
fn row(entry: &Entry, width: usize, widest_place: usize, theme: &Theme) -> Line<'static> {
    let columns = width.saturating_sub(2);
    let place_width = widest_place.min(columns / 2);
    let place = fit(&entry.place, place_width);
    let gap = " ".repeat(place_width.saturating_sub(place.width()) + 2);
    let text = fit(&entry.text, columns.saturating_sub(place_width + 2));
    Line::from(vec![
        Span::raw(" "),
        Span::styled(place, theme.style("completion.kind")),
        Span::raw(gap),
        Span::raw(text),
    ])
}
