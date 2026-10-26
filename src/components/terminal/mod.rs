//! The integrated terminal pane: a tab for each shell, the active one's screen
//! drawn inside a frame, and the size every shell is told to have.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};

use crate::app::state::AppState;
use crate::pty::Session;
use crate::ui::Render;
use crate::ui::layout::Placement;

pub(crate) mod links;
mod render;
#[cfg(test)]
mod tests;

/// Tabs beyond this are refused, so a runaway script cannot start shells
/// without end.
pub(crate) const MAX_TABS: usize = 16;

pub(crate) use links::{Link, LinkText};

/// The pane and the shells running in it.
#[derive(Debug, Default)]
pub(crate) struct TerminalPane {
    tabs: Vec<Tab>,
    active: usize,
    /// Rows and columns of the text area, as of the last layout pass. Kept
    /// while the pane is hidden, so the shells keep the size they last had.
    size: Option<(u16, u16)>,
    /// Where the pane, its tab bar and its text are on screen, as of the last
    /// layout pass.
    area: Rect,
    tab_bar: Rect,
    text_area: Rect,
    tab_rects: Vec<Rect>,
    /// The "+" that opens a tab, right after the last tab in the bar. Empty
    /// when there is no room.
    add_button: Rect,
    /// What the user selected with the mouse, to copy.
    selection: Option<Selection>,
    /// The path the pointer is over with `ctrl` held, which is underlined.
    link: Option<links::LinkMark>,
}

/// A cell of a shell's screen, counted from the screen's top left corner. Cells order in
/// reading order (row first), which is what a selection needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Cell {
    pub(crate) row: u16,
    pub(crate) column: u16,
}

/// The pair as `(row, column)`, the order a terminal screen is indexed in.
impl From<(u16, u16)> for Cell {
    fn from((row, column): (u16, u16)) -> Self {
        Self { row, column }
    }
}

/// Text selected on a tab's screen: from the cell the press was on to the cell the pointer
/// is on, as the screen is drawn (so with the history offset it had). It is dropped when the
/// text under it changes, the view moves or another tab is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Selection {
    tab: usize,
    /// Where the press was and where the pointer is.
    anchor: Cell,
    head: Cell,
    offset: usize,
    /// Whether anything is selected: the pointer left the cell it pressed on, or a word or a
    /// line was taken. A press alone selects nothing.
    armed: bool,
    /// The text as of the last change, which is also what is compared with.
    text: String,
}

/// The characters that make a word for a double click: letters, digits and what a path or
/// an option has in it.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '~' | ':' | '@' | '%' | '+' | '=')
}

#[derive(Debug)]
struct Tab {
    session: Session,
    /// The program, for the tab bar.
    title: String,
    /// The end of the shell has been reported to the user.
    exit_reported: bool,
    /// The run configuration this tab is the tab of (see `app/run.rs`); a shell has none.
    run: Option<String>,
}

/// A shell or program that ended, as [`TerminalPane::take_exits`] tells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TabExit {
    /// The tab's number, from 1.
    pub(crate) number: usize,
    pub(crate) code: u32,
    pub(crate) title: String,
    /// The run configuration of the tab, when it is a run's.
    pub(crate) run: Option<String>,
}

impl Tab {
    fn is_running(&self) -> bool {
        self.session.exit_code().is_none() && !self.session.output_ended()
    }
}

impl TerminalPane {
    /// Adds a tab for `session` and makes it the active one.
    pub(crate) fn start(&mut self, session: Session, title: String) {
        self.tabs.push(Tab {
            session,
            title,
            exit_reported: false,
            run: None,
        });
        self.active = self.tabs.len() - 1;
    }

    /// Adds a tab for the run configuration `run` and makes it the active one.
    pub(crate) fn start_run(&mut self, session: Session, title: String, run: String) {
        self.start(session, title);
        if let Some(tab) = self.tabs.last_mut() {
            tab.run = Some(run);
        }
    }

    /// The tab of the run configuration `run`.
    pub(crate) fn run_tab(&self, run: &str) -> Option<usize> {
        self.tabs
            .iter()
            .position(|tab| tab.run.as_deref() == Some(run))
    }

    /// Whether the tab of `run` has a program running (or a build, which is shown there too).
    pub(crate) fn run_is_running(&self, run: &str) -> bool {
        self.run_tab(run)
            .and_then(|index| self.tabs.get(index))
            .is_some_and(Tab::is_running)
    }

    /// Gives tab `index` a new session, which stops the old one, and makes the tab the active
    /// one. Its end is to be reported again.
    pub(crate) fn replace_session(&mut self, index: usize, session: Session, title: String) {
        let Some(tab) = self.tabs.get_mut(index) else {
            return;
        };
        tab.session = session;
        tab.title = title;
        tab.exit_reported = false;
        self.active = index;
        self.selection = None;
    }

    /// The active tab's shell.
    pub(crate) fn session(&self) -> Option<&Session> {
        self.tabs.get(self.active).map(|tab| &tab.session)
    }

    /// The shell of tab `index`.
    pub(crate) fn session_of(&self, index: usize) -> Option<&Session> {
        self.tabs.get(index).map(|tab| &tab.session)
    }

    pub(crate) fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub(crate) fn active_tab(&self) -> usize {
        self.active
    }

    /// Whether the active tab's shell has not ended.
    pub(crate) fn is_running(&self) -> bool {
        self.tabs.get(self.active).is_some_and(Tab::is_running)
    }

    /// Whether any tab's shell has not ended.
    pub(crate) fn any_running(&self) -> bool {
        self.tabs.iter().any(Tab::is_running)
    }

    /// Drops every tab whose shell has ended.
    pub(crate) fn remove_ended(&mut self) {
        let mut kept = Vec::new();
        let mut kept_before_active = 0;
        for (index, tab) in std::mem::take(&mut self.tabs).into_iter().enumerate() {
            if !tab.is_running() {
                continue;
            }
            if index < self.active {
                kept_before_active += 1;
            }
            kept.push(tab);
        }
        self.active = kept_before_active.min(kept.len().saturating_sub(1));
        self.tabs = kept;
    }

    pub(crate) fn select(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
        }
    }

    pub(crate) fn select_next(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
        }
    }

    pub(crate) fn select_previous(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
        }
    }

    /// Closes tab `index`, which stops its shell. The tab before it (or the
    /// one after, for the first) becomes active if it was the active one.
    pub(crate) fn close(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.tabs.remove(index);
        if index < self.active || (index == self.active && self.active > 0) {
            self.active -= 1;
        }
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
    }

    /// The tab whose label is at a screen cell.
    pub(crate) fn tab_at(&self, column: u16, row: u16) -> Option<usize> {
        let position = Position::new(column, row);
        self.tab_rects
            .iter()
            .position(|rect| rect.width > 0 && rect.contains(position))
    }

    /// Whether a screen cell is on the "+" button that opens a tab.
    pub(crate) fn is_add_button(&self, column: u16, row: u16) -> bool {
        self.add_button.width > 0 && self.add_button.contains(Position::new(column, row))
    }

    /// Where the pane is on screen, as of the last layout pass.
    pub(crate) fn area(&self) -> Rect {
        self.area
    }

    /// Whether a screen cell is inside the pane, as of the last layout pass.
    pub(crate) fn contains(&self, column: u16, row: u16) -> bool {
        self.area.contains(Position::new(column, row))
    }

    /// The cell of the shell's screen under a screen cell; `None` outside the text (on the
    /// frame, say, or the tab bar).
    pub(crate) fn cell_at(&self, column: u16, row: u16) -> Option<Cell> {
        self.text_area
            .contains(Position::new(column, row))
            .then(|| Cell {
                row: row - self.text_area.y,
                column: column - self.text_area.x,
            })
    }

    /// [`Self::cell_at`] as `(column, row)`.
    pub(crate) fn screen_cell(&self, column: u16, row: u16) -> Option<(u16, u16)> {
        let cell = self.cell_at(column, row)?;
        Some((cell.column, cell.row))
    }

    /// The cell of the shell's screen nearest to a screen cell, for a drag that went outside
    /// the text: the edge it left by.
    pub(crate) fn clamped_cell(&self, column: u16, row: u16) -> Option<Cell> {
        let area = self.text_area;
        if area.width == 0 || area.height == 0 {
            return None;
        }
        Some(Cell {
            row: row.clamp(area.y, area.bottom() - 1) - area.y,
            column: column.clamp(area.x, area.right() - 1) - area.x,
        })
    }

    /// The selected text as of now, if any characters are selected.
    pub(crate) fn selected_text(&self) -> Option<&str> {
        self.selection
            .as_ref()
            .map(|selection| selection.text.as_str())
            .filter(|text| !text.is_empty())
    }

    /// Drops the selection. Returns whether there was one.
    pub(crate) fn clear_selection(&mut self) -> bool {
        self.selection.take().is_some()
    }

    /// Starts a selection at `cell`: nothing is selected until it is extended. A tuple is
    /// `(row, column)`.
    pub(crate) fn begin_selection(&mut self, cell: impl Into<Cell>) {
        let cell = cell.into();
        let Some(session) = self.session() else {
            return;
        };
        let offset = session.scrollback_offset();
        self.selection = Some(Selection {
            tab: self.active,
            anchor: cell,
            head: cell,
            offset,
            armed: false,
            text: String::new(),
        });
    }

    /// Moves the end of the selection to `cell`.
    pub(crate) fn extend_selection(&mut self, cell: impl Into<Cell>) {
        let cell = cell.into();
        let Some(selection) = self.selection.as_mut() else {
            return;
        };
        selection.head = cell;
        selection.armed |= cell != selection.anchor;
        self.refresh_selection_text();
    }

    /// Selects the word at `cell` (a double click).
    pub(crate) fn select_word(&mut self, cell: impl Into<Cell>) {
        let Cell { row, column } = cell.into();
        let Some(session) = self.session() else {
            return;
        };
        let range = session.with_screen(|screen| {
            let at = |c: u16| {
                screen
                    .cell(row, c)
                    .and_then(|cell| cell.contents().chars().next())
                    .is_some_and(is_word_char)
            };
            if !at(column) {
                return (column, column);
            }
            let columns = screen.size().1;
            let mut start = column;
            while start > 0 && at(start - 1) {
                start -= 1;
            }
            let mut end = column;
            while end + 1 < columns && at(end + 1) {
                end += 1;
            }
            (start, end)
        });
        self.begin_selection(Cell {
            row,
            column: range.0,
        });
        self.extend_selection(Cell {
            row,
            column: range.1,
        });
        self.arm_selection();
    }

    /// Makes the selection count even when it is one cell, as a word of one letter is.
    fn arm_selection(&mut self) {
        if let Some(selection) = self.selection.as_mut() {
            selection.armed = true;
        }
        self.refresh_selection_text();
    }

    /// Selects the whole row at `cell` (a triple click).
    pub(crate) fn select_line(&mut self, cell: impl Into<Cell>) {
        let Cell { row, .. } = cell.into();
        let Some(columns) = self
            .session()
            .map(|s| s.with_screen(|screen| screen.size().1))
        else {
            return;
        };
        self.begin_selection(Cell { row, column: 0 });
        self.extend_selection(Cell {
            row,
            column: columns.saturating_sub(1),
        });
        self.arm_selection();
    }

    /// The selection as (first, last) cells in reading order, both included.
    fn selection_range(&self) -> Option<(Cell, Cell)> {
        let selection = self.selection.as_ref()?;
        let (a, b) = (selection.anchor, selection.head);
        Some(if a <= b { (a, b) } else { (b, a) })
    }

    fn refresh_selection_text(&mut self) {
        let Some((first, last)) = self.selection_range() else {
            return;
        };
        if !self
            .selection
            .as_ref()
            .is_some_and(|selection| selection.armed)
        {
            return;
        }
        let text = self.session().map(|session| {
            session.with_screen(|screen| {
                let columns = screen.size().1;
                screen.contents_between(
                    first.row,
                    first.column,
                    last.row,
                    last.column.saturating_add(1).min(columns),
                )
            })
        });
        if let (Some(selection), Some(text)) = (self.selection.as_mut(), text) {
            selection.text = text;
        }
    }

    /// Drops the selection when what it was made of is not there any more: another tab,
    /// the view moved into the history, or the text under it changed.
    fn check_selection(&mut self) {
        let Some(selection) = &self.selection else {
            return;
        };
        let same_place = selection.tab == self.active
            && self.session().map(Session::scrollback_offset) == Some(selection.offset);
        let before = selection.text.clone();
        if !same_place {
            self.selection = None;
            return;
        }
        self.refresh_selection_text();
        if self.selection.as_ref().map(|s| s.text.as_str()) != Some(before.as_str()) {
            self.selection = None;
        }
    }

    /// The size the shells have, or will have once the pane is drawn.
    pub(crate) fn size(&self) -> Option<(u16, u16)> {
        self.size
    }

    /// Shells that ended since the last call, each once.
    pub(crate) fn take_exits(&mut self) -> Vec<TabExit> {
        let mut exits = Vec::new();
        for (index, tab) in self.tabs.iter_mut().enumerate() {
            if tab.exit_reported {
                continue;
            }
            if let Some(code) = tab.session.finished() {
                tab.exit_reported = true;
                exits.push(TabExit {
                    number: index + 1,
                    code,
                    title: tab.title.clone(),
                    run: tab.run.clone(),
                });
            }
        }
        exits
    }
}

impl Render<AppState> for TerminalPane {
    fn prepare(&mut self, placement: &Placement) {
        self.area = placement.area;
        let inner = placement.frame.inner(placement.area);
        // The tab bar is always there while there is a tab, so the "+" is too.
        let bar = u16::from(!self.tabs.is_empty()).min(inner.height);
        self.tab_bar = Rect {
            height: bar,
            ..inner
        };
        self.text_area = Rect {
            y: inner.y + bar,
            height: inner.height - bar,
            ..inner
        };
        // The labels leave room for the button at the end of the bar.
        let labels_room = Rect {
            width: self.tab_bar.width.saturating_sub(ADD_BUTTON_WIDTH),
            ..self.tab_bar
        };
        self.tab_rects = render::tab_rects(&self.labels(), labels_room);
        self.add_button = add_button_after(&self.tab_rects, self.tab_bar);

        self.check_selection();
        let size = (self.text_area.height, self.text_area.width);
        let resize = self.size != Some(size) && size.0 > 0 && size.1 > 0;
        for tab in &self.tabs {
            // The screen is about to be drawn: whatever it shows now is seen,
            // so the next change wakes the app again.
            tab.session.take_dirty();
            if resize {
                tab.session.resize(size.0, size.1);
            }
        }
        self.size = Some(size);
    }

    fn render(&self, frame: &mut Frame, state: &AppState, placement: &Placement) {
        render::draw(frame, state, placement, self);
    }
}

impl TerminalPane {
    /// What each tab's label says.
    fn labels(&self) -> Vec<String> {
        self.tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let ended = if tab.is_running() { "" } else { " (ended)" };
                format!(" {}: {}{ended} ", index + 1, tab.title)
            })
            .collect()
    }
}

/// Width of the "+" button, with a space on each side.
const ADD_BUTTON_WIDTH: u16 = 3;

/// Where the "+" button goes: right after the last tab, or at the end of the bar
/// when the tabs fill it. An empty rectangle when the bar has no room for it.
fn add_button_after(tab_rects: &[Rect], bar: Rect) -> Rect {
    if bar.height == 0 || bar.width < ADD_BUTTON_WIDTH {
        return Rect::default();
    }
    let after_tabs = tab_rects.last().map_or(bar.x, |last| last.right());
    let x = after_tabs.min(bar.right() - ADD_BUTTON_WIDTH);
    Rect::new(x, bar.y, ADD_BUTTON_WIDTH, 1)
}
