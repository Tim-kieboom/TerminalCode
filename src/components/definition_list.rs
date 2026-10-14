//! The places to choose from when a server names several definitions (a trait
//! method has one for each implementation): a box in the upper middle of the
//! screen, a row for each place with its file, its line number and the text of
//! that line. Up and down move, Enter goes, Esc closes; a click on a row goes.

use std::cell::Cell;
use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::components::editor::render::fit;
use crate::lsp::{Location, ServerKey};
use crate::ui::theme::Theme;

/// How many rows show at once; the rest scrolls.
const MOST_ROWS: usize = 12;
/// The box is at least this wide when the screen allows, and at most [`WIDEST`].
const NARROWEST: u16 = 40;
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
    entries: Vec<Entry>,
    /// The server that named the places: the files outside the project stay with it.
    key: ServerKey,
    selected: usize,
    /// The first row shown.
    scroll: Cell<usize>,
    /// Where it was last drawn, for clicks; empty until the first draw.
    area: Cell<Rect>,
}

impl DefinitionList {
    pub(crate) fn new(entries: Vec<Entry>, key: ServerKey) -> Self {
        Self {
            entries,
            key,
            selected: 0,
            scroll: Cell::new(0),
            area: Cell::new(Rect::default()),
        }
    }

    pub(crate) fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub(crate) fn key(&self) -> &ServerKey {
        &self.key
    }

    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    pub(crate) fn selected_location(&self) -> Option<&Location> {
        self.entries.get(self.selected).map(|entry| &entry.location)
    }

    /// Moves the selection by `rows`, round the ends.
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

    /// Moves by a page.
    pub(crate) fn page(&mut self, pages: isize) {
        self.step(pages * isize::try_from(MOST_ROWS).unwrap_or(1));
    }

    /// Selects the row at screen row `row`, if it is one. Whether it was.
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

        let title = format!(" Definitions ({}) ", self.entries.len());
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border.focused"))
            .title(Line::styled(title, theme.style("pane.title")));
        let inner = block.inner(area);
        crate::ui::backdrop::clear(frame, theme, area);
        frame.render_widget(block, area);

        let columns = usize::from(inner.width).saturating_sub(2);
        let place_width = self
            .entries
            .iter()
            .map(|entry| entry.place.width())
            .max()
            .unwrap_or(0)
            .min(columns / 2);
        let lines: Vec<Line> = self
            .entries
            .iter()
            .enumerate()
            .skip(first)
            .take(rows)
            .map(|(index, entry)| {
                let place = fit(&entry.place, place_width);
                let gap = " ".repeat(place_width.saturating_sub(place.width()) + 2);
                let text = fit(&entry.text, columns.saturating_sub(place_width + 2));
                let line = Line::from(vec![
                    Span::raw(" "),
                    Span::styled(place, theme.style("completion.kind")),
                    Span::raw(gap),
                    Span::raw(text),
                ]);
                match index == self.selected {
                    true => line.style(theme.style("completion.selected")),
                    false => line,
                }
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

/// `path` as short as is useful: relative to the project when it is in it, else
/// the last three parts.
pub(crate) fn short_path(path: &Path, project: &Path) -> String {
    if let Ok(relative) = path.strip_prefix(project) {
        return slashes(&relative.display().to_string());
    }
    let parts: Vec<_> = path.components().collect();
    let skip = parts.len().saturating_sub(3);
    let kept: std::path::PathBuf = parts.into_iter().skip(skip).collect();
    match skip {
        0 => slashes(&kept.display().to_string()),
        _ => format!("…/{}", slashes(&kept.display().to_string())),
    }
}

/// `path` with `/` between the parts, on every system.
fn slashes(path: &str) -> String {
    path.replace('\\', "/")
}
