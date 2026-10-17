//! The integrated terminal pane: a tab for each shell, the active one's screen
//! drawn inside a frame, and the size every shell is told to have.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};

use crate::app::state::AppState;
use crate::pty::Session;
use crate::ui::Render;
use crate::ui::layout::Placement;

mod render;
#[cfg(test)]
mod tests;

/// Tabs beyond this are refused, so a runaway script cannot start shells
/// without end.
pub(crate) const MAX_TABS: usize = 16;

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
}

#[derive(Debug)]
struct Tab {
    session: Session,
    /// The program, for the tab bar.
    title: String,
    /// The end of the shell has been reported to the user.
    exit_reported: bool,
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
        });
        self.active = self.tabs.len() - 1;
    }

    /// The active tab's shell.
    pub(crate) fn session(&self) -> Option<&Session> {
        self.tabs.get(self.active).map(|tab| &tab.session)
    }

    /// The shell of tab `index`, for tests.
    #[cfg(test)]
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

    /// The cell of the shell's screen under a screen cell, counted from the
    /// screen's top left corner; `None` outside the text (on the frame, say, or
    /// the tab bar).
    pub(crate) fn screen_cell(&self, column: u16, row: u16) -> Option<(u16, u16)> {
        self.text_area
            .contains(Position::new(column, row))
            .then(|| (column - self.text_area.x, row - self.text_area.y))
    }

    /// The size the shells have, or will have once the pane is drawn.
    pub(crate) fn size(&self) -> Option<(u16, u16)> {
        self.size
    }

    /// Shells that ended since the last call, each once: the tab's number
    /// (from 1) and the exit code.
    pub(crate) fn take_exits(&mut self) -> Vec<(usize, u32)> {
        let mut exits = Vec::new();
        for (index, tab) in self.tabs.iter_mut().enumerate() {
            if tab.exit_reported {
                continue;
            }
            if let Some(code) = tab.session.finished() {
                tab.exit_reported = true;
                exits.push((index + 1, code));
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
