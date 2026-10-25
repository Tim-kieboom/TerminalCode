//! The code actions a server offers at the cursor: a box in the upper middle of
//! the screen with a row for each, numbered for the first nine. Up and down
//! move, Enter or a number does it, Esc closes; a click on a row does it. The picker of
//! run configurations is the same box with another title.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::pick_list::PickList;
use super::plain_key;
use crate::components::text::fit;
use crate::ui::theme::Theme;

/// The box is at most this wide.
const WIDEST: u16 = 100;

/// What a key press asks the list to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Previous,
    Next,
    PageUp,
    PageDown,
    Go,
    /// The action with this number (from 1).
    Pick(usize),
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
            KeyCode::Char(digit @ '1'..='9') => Self::Pick(digit as usize - '0' as usize),
            _ => return None,
        })
    }
}

/// One action, as a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) title: String,
    /// `quickfix`, `refactor.extract`, and so on; empty when the server says none.
    pub(crate) kind: String,
    /// Why the server says the action cannot be done now: it is listed, dim, with this.
    pub(crate) disabled: Option<String>,
}

#[derive(Debug)]
pub(crate) struct ActionList {
    list: PickList<Entry>,
    /// What the box is called, when it is not "Code actions".
    title: Option<String>,
}

impl ActionList {
    pub(crate) fn new(entries: Vec<Entry>) -> Self {
        Self {
            list: PickList::new(entries),
            title: None,
        }
    }

    /// The box with another name, and the row `selected` selected.
    pub(crate) fn titled(entries: Vec<Entry>, title: &str, selected: usize) -> Self {
        Self {
            list: PickList::new(entries).selecting(selected),
            title: Some(title.to_owned()),
        }
    }

    #[cfg(test)]
    pub(crate) fn entries(&self) -> &[Entry] {
        self.list.entries()
    }

    pub(crate) fn selected(&self) -> usize {
        self.list.selected()
    }

    /// Why the selected action cannot be done, if it cannot.
    pub(crate) fn selected_disabled(&self) -> Option<&str> {
        self.list.selected_entry()?.disabled.as_deref()
    }

    /// Moves the selection by `rows`, round the ends for a step of one.
    pub(crate) fn step(&mut self, rows: isize) {
        self.list.step(rows);
    }

    pub(crate) fn page(&mut self, pages: isize) {
        self.list.page(pages);
    }

    /// Selects the action with this number (from 1), if there is one.
    pub(crate) fn select_number(&mut self, number: usize) -> bool {
        match number.checked_sub(1) {
            Some(index) => self.list.select_index(index),
            None => false,
        }
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
        let name = self.title.as_deref().unwrap_or("Code actions");
        self.list
            .draw(frame, theme, name, WIDEST, |entry, index, width| {
                row(entry, index, width, theme)
            });
    }
}

/// The row of one action: its number, its title, and its kind or why it is disabled.
fn row(entry: &Entry, index: usize, width: usize, theme: &Theme) -> Line<'static> {
    let columns = width.saturating_sub(4);
    let number = match index < 9 {
        true => format!(" {} ", index + 1),
        false => "   ".to_owned(),
    };
    let title = fit(&entry.title, columns);
    let room = columns.saturating_sub(title.width() + 2);
    // A disabled action says why instead of what kind it is.
    let note = match &entry.disabled {
        Some(reason) => format!("(disabled: {reason})"),
        None => entry.kind.clone(),
    };
    let note = fit(&note, room);
    let faint = theme.style("tab.inactive");
    let mut spans = vec![
        Span::styled(number, theme.style("completion.kind")),
        match entry.disabled {
            Some(_) => Span::styled(title, faint),
            None => Span::raw(title),
        },
    ];
    if !note.is_empty() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            note,
            match entry.disabled {
                Some(_) => faint,
                None => theme.style("completion.kind"),
            },
        ));
    }
    Line::from(spans)
}
