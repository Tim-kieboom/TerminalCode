//! The command palette: a searchable list of every action.
//!
//! Typing narrows the list with a fuzzy match on the action's title; enter
//! runs the selected one. Each entry shows the keys it is bound to.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::event::action::Action;
use crate::ui::fuzzy;
use crate::ui::theme::Theme;

const WIDTH: u16 = 64;
const MAX_ROWS: u16 = 12;
const NO_MATCH: &str = " no matching command";

/// One action in the palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) title: &'static str,
    pub(crate) action: Action,
    /// The keys bound to the action, if any, such as `ctrl+s`.
    pub(crate) keys: Option<String>,
}

/// What a key press asks the palette to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Insert(char),
    Backspace,
    ClearQuery,
    Previous,
    Next,
    Run,
    Cancel,
}

impl Command {
    pub(crate) fn from_key(key: KeyEvent) -> Option<Self> {
        if key.kind == KeyEventKind::Release || key.modifiers.contains(KeyModifiers::ALT) {
            return None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        Some(match (key.code, ctrl) {
            (KeyCode::Esc, _) => Self::Cancel,
            (KeyCode::Enter, _) => Self::Run,
            (KeyCode::Up, _) => Self::Previous,
            (KeyCode::Down, _) => Self::Next,
            (KeyCode::Backspace, _) => Self::Backspace,
            (KeyCode::Char('u'), true) => Self::ClearQuery,
            (KeyCode::Char(c), false) => Self::Insert(c),
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Palette {
    entries: Vec<Entry>,
    query: String,
    /// Indices into `entries` that match the query, best first.
    matches: Vec<usize>,
    selected: usize,
}

impl Palette {
    pub(crate) fn new(entries: Vec<Entry>) -> Self {
        let mut palette = Self {
            entries,
            query: String::new(),
            matches: Vec::new(),
            selected: 0,
        };
        palette.refilter();
        palette
    }

    #[cfg(test)]
    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    /// The entries that match the query, best first.
    pub(crate) fn visible(&self) -> impl Iterator<Item = &Entry> {
        self.matches.iter().map(|&index| &self.entries[index])
    }

    pub(crate) fn selected_entry(&self) -> Option<&Entry> {
        self.matches
            .get(self.selected)
            .map(|&index| &self.entries[index])
    }

    #[cfg(test)]
    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    pub(crate) fn insert(&mut self, c: char) {
        self.query.push(c);
        self.refilter();
    }

    pub(crate) fn backspace(&mut self) {
        self.query.pop();
        self.refilter();
    }

    pub(crate) fn clear_query(&mut self) {
        self.query.clear();
        self.refilter();
    }

    pub(crate) fn select_previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub(crate) fn select_next(&mut self) {
        self.selected = (self.selected + 1).min(self.matches.len().saturating_sub(1));
    }

    /// Recomputes the matches, best first; equal scores keep the order of the
    /// full list. The selection goes back to the best match.
    fn refilter(&mut self) {
        let mut scored: Vec<(i32, usize)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| Some((fuzzy::score(&self.query, entry.title)?, index)))
            .collect();
        scored.sort_by_key(|&(score, index)| (std::cmp::Reverse(score), index));
        self.matches = scored.into_iter().map(|(_, index)| index).collect();
        self.selected = 0;
    }

    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        let area = self.area(frame.area());
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border.focused"))
            .title(Line::styled(" Command Palette ", theme.style("pane.title")));
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);
        if inner.height == 0 {
            return;
        }

        let input_area = Rect { height: 1, ..inner };
        let prompt = format!("> {}", self.query);
        frame.render_widget(Paragraph::new(prompt.as_str()), input_area);
        let cursor_x = input_area.x + prompt.width().min(usize::from(input_area.width)) as u16;
        frame.set_cursor_position(Position::new(
            cursor_x.min(input_area.right()),
            input_area.y,
        ));

        let list_area = Rect {
            y: inner.y + 1,
            height: inner.height - 1,
            ..inner
        };
        if self.matches.is_empty() {
            let style = theme.style("pane.border");
            frame.render_widget(Paragraph::new(NO_MATCH).style(style), list_area);
            return;
        }
        let rows = self.visible().map(|entry| row(entry, inner.width, theme));
        let mut state = ListState::default().with_selected(Some(self.selected));
        let list = List::new(rows).highlight_style(theme.style("list.selected"));
        frame.render_stateful_widget(list, list_area, &mut state);
    }

    /// The palette's box: horizontally centered, near the top.
    fn area(&self, screen: Rect) -> Rect {
        let width = WIDTH.min(screen.width);
        let rows = (self.matches.len().max(1) as u16).min(MAX_ROWS);
        let height = (rows + 3).min(screen.height);
        Rect {
            x: screen.x + (screen.width - width) / 2,
            y: screen.y + (screen.height / 8).min(screen.height - height),
            width,
            height,
        }
    }
}

/// A list row: the title on the left, its keys on the right.
fn row<'a>(entry: &Entry, width: u16, theme: &Theme) -> ListItem<'a> {
    let width = usize::from(width);
    let title = format!(" {}", entry.title);
    let Some(keys) = &entry.keys else {
        return ListItem::new(Line::from(title));
    };
    let keys = format!("{keys} ");
    let gap = width.saturating_sub(title.width() + keys.width()).max(1);
    let dim: Style = theme.style("pane.border");
    ListItem::new(Line::from(vec![
        title.into(),
        " ".repeat(gap).into(),
        ratatui::text::Span::styled(keys, dim),
    ]))
}
