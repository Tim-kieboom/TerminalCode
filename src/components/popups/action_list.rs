//! The code actions a server offers at the cursor: a box in the upper middle of
//! the screen with a row for each, numbered for the first nine. Up and down
//! move, Enter or a number does it, Esc closes; a click on a row does it. The picker of
//! run configurations is the same box with another title.

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::components::editor::render::fit;
use crate::ui::theme::Theme;

/// How many rows show at once; the rest scrolls.
const MOST_ROWS: usize = 12;
/// The box is at least this wide when the screen allows, and at most [`WIDEST`].
const NARROWEST: u16 = 40;
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
        if key.kind == KeyEventKind::Release
            || key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return None;
        }
        Some(match key.code {
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
    entries: Vec<Entry>,
    /// What the box is called, when it is not "Code actions".
    title: Option<String>,
    selected: usize,
    /// The first row shown.
    scroll: Cell<usize>,
    /// Where it was last drawn, for clicks; empty until the first draw.
    area: Cell<Rect>,
}

impl ActionList {
    pub(crate) fn new(entries: Vec<Entry>) -> Self {
        Self {
            entries,
            title: None,
            selected: 0,
            scroll: Cell::new(0),
            area: Cell::new(Rect::default()),
        }
    }

    /// The box with another name, and the row `selected` selected.
    pub(crate) fn titled(entries: Vec<Entry>, title: &str, selected: usize) -> Self {
        let mut list = Self::new(entries);
        list.title = Some(title.to_owned());
        list.selected = selected.min(list.entries.len().saturating_sub(1));
        list
    }

    #[cfg(test)]
    pub(crate) fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    /// Why the selected action cannot be done, if it cannot.
    pub(crate) fn selected_disabled(&self) -> Option<&str> {
        self.entries.get(self.selected)?.disabled.as_deref()
    }

    /// Moves the selection by `rows`, round the ends for a step of one.
    pub(crate) fn step(&mut self, rows: isize) {
        let count = isize::try_from(self.entries.len()).unwrap_or(isize::MAX);
        if count == 0 {
            return;
        }
        let at = isize::try_from(self.selected).unwrap_or(0);
        let moved = at + rows;
        self.selected = match rows.abs() {
            1 => moved.rem_euclid(count),
            _ => moved.clamp(0, count - 1),
        } as usize;
    }

    pub(crate) fn page(&mut self, pages: isize) {
        self.step(pages * isize::try_from(MOST_ROWS).unwrap_or(1));
    }

    /// Selects the action with this number (from 1), if there is one.
    pub(crate) fn select_number(&mut self, number: usize) -> bool {
        match number.checked_sub(1).filter(|&at| at < self.entries.len()) {
            Some(at) => {
                self.selected = at;
                true
            }
            None => false,
        }
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
        if index >= self.entries.len() {
            return false;
        }
        self.selected = index;
        true
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

    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        let screen = frame.area();
        let width = WIDEST
            .min(screen.width.saturating_sub(4))
            .max(NARROWEST.min(screen.width));
        let rows = self.entries.len().min(MOST_ROWS);
        let height = u16::try_from(rows).unwrap_or(u16::MAX) + 2;
        if width < 8 || screen.height < height {
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
        let mut first = self.scroll.get();
        if self.selected < first {
            first = self.selected;
        } else if self.selected >= first + rows {
            first = self.selected + 1 - rows;
        }
        first = first.min(self.entries.len().saturating_sub(rows));
        self.scroll.set(first);

        let name = self.title.as_deref().unwrap_or("Code actions");
        let title = format!(" {name} ({}) ", self.entries.len());
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border.focused"))
            .title(Line::styled(title, theme.style("pane.title")));
        let inner = block.inner(area);
        crate::ui::backdrop::clear(frame, theme, area);
        frame.render_widget(block, area);

        let columns = usize::from(inner.width).saturating_sub(4);
        let lines: Vec<Line> = self
            .entries
            .iter()
            .enumerate()
            .skip(first)
            .take(rows)
            .map(|(index, entry)| {
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
                let line = Line::from(spans);
                match index == self.selected {
                    true => line.style(theme.style("completion.selected")),
                    false => line,
                }
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), inner);
    }
}
