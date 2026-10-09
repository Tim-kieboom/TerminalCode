//! The "unsaved changes" prompt shown when quitting with modified documents.
//!
//! It lists every modified document. Each one can be saved or discarded on
//! its own, or all at once; the app quits when none are left to decide on.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph};

use crate::app::state::AppState;
use crate::components::workspace::DocumentId;
use crate::ui::Render;
use crate::ui::layout::Placement;
use crate::ui::theme::Theme;

const HELP: [&str; 3] = [
    "esc:    cancel",
    "s:      save all | d:      discard all",
    "ctrl+s: save     | ctrl+d: discard",
];
const MIN_WIDTH: u16 = 46;
/// A name and the rule under it.
const ROWS_PER_ITEM: u16 = 2;

/// A modified document the user has to decide about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    pub(crate) document: DocumentId,
    pub(crate) name: String,
}

/// What a key press asks the prompt to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Previous,
    Next,
    Save,
    Discard,
    SaveAll,
    DiscardAll,
    Cancel,
}

impl Command {
    /// The command a key stands for, if any. Plain `s` and `d` act on every
    /// file, ctrl+s and ctrl+d on the selected one.
    pub(crate) fn from_key(key: KeyEvent) -> Option<Self> {
        if key.kind == KeyEventKind::Release || key.modifiers.contains(KeyModifiers::ALT) {
            return None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        Some(match (key.code, ctrl) {
            (KeyCode::Up, false) => Self::Previous,
            (KeyCode::Down, false) => Self::Next,
            (KeyCode::Esc, false) => Self::Cancel,
            (KeyCode::Char('s' | 'S'), false) => Self::SaveAll,
            (KeyCode::Char('d' | 'D'), false) => Self::DiscardAll,
            (KeyCode::Char('s'), true) => Self::Save,
            (KeyCode::Char('d'), true) => Self::Discard,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct QuitPrompt {
    items: Vec<Item>,
    selected: usize,
}
impl Render for QuitPrompt {
    fn render(&self, frame: &mut Frame, state: &AppState, _: &Placement) {
        self.render(frame, state.theme());
    }
}
impl QuitPrompt {
    pub(crate) fn new(items: Vec<Item>) -> Self {
        Self { items, selected: 0 }
    }

    #[cfg(test)]
    pub(crate) fn items(&self) -> &[Item] {
        &self.items
    }

    #[cfg(test)]
    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    pub(crate) fn selected_item(&self) -> Option<&Item> {
        self.items.get(self.selected)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub(crate) fn select_previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub(crate) fn select_next(&mut self) {
        self.selected = (self.selected + 1).min(self.items.len().saturating_sub(1));
    }

    /// Drops the selected document from the list; the selection stays on the
    /// entry that took its place.
    pub(crate) fn remove_selected(&mut self) {
        if self.selected < self.items.len() {
            self.items.remove(self.selected);
        }
        self.selected = self.selected.min(self.items.len().saturating_sub(1));
    }

    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        let area = self.area(frame.area());
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border.focused"))
            .title(Line::styled(" Unsaved changes ", theme.style("pane.title")));

        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);

        let help_height = HELP.len() as u16;
        let list_height = inner.height.saturating_sub(help_height);
        let list_area = Rect {
            height: list_height,
            ..inner
        };
        let help_area = Rect {
            y: inner.y + list_height,
            height: inner.height - list_height,
            ..inner
        };

        let rule = Line::styled(
            "─".repeat(usize::from(inner.width)),
            theme.style("pane.border"),
        );
        let items = self.items.iter().enumerate().map(|(index, item)| {
            let style = match index == self.selected {
                true => theme.style("list.selected"),
                false => Default::default(),
            };
            let name = Line::styled(format!(" {} ", item.name), style);
            ListItem::new(vec![name, rule.clone()])
        });
        let mut state = ListState::default().with_selected(Some(self.selected));
        frame.render_stateful_widget(List::new(items), list_area, &mut state);

        let help: Vec<Line> = HELP.iter().map(|line| Line::from(*line)).collect();
        frame.render_widget(Paragraph::new(help), help_area);
    }

    /// The prompt's box: centered, wide enough for the longest name.
    fn area(&self, screen: Rect) -> Rect {
        let longest = self
            .items
            .iter()
            .map(|item| item.name.chars().count())
            .max()
            .unwrap_or(0);

        let width = (longest as u16 + 6).max(MIN_WIDTH).min(screen.width);
        let rows =
            (self.items.len() as u16 * ROWS_PER_ITEM + HELP.len() as u16 + 2).min(screen.height);

        Rect {
            x: screen.x + (screen.width - width) / 2,
            y: screen.y + (screen.height - rows) / 2,
            width,
            height: rows,
        }
    }
}
