//! A context menu: a short list of actions in a small box beside what it is
//! about. Up and down move over the items, Enter runs one, Esc closes it.
//!
//! The menu does not know what its actions do. It remembers the path it was
//! opened on, so they act on that and not on whatever is selected later.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::event::action::Action;
use crate::ui::theme::Theme;

/// One line of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Entry {
    Item {
        label: &'static str,
        action: Action,
    },
    /// A rule between two groups of items; the selection skips it.
    Separator,
}

/// What a key press asks the menu to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Previous,
    Next,
    Run,
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
            KeyCode::Enter => Self::Run,
            KeyCode::Esc => Self::Cancel,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Menu {
    entries: Vec<Entry>,
    /// Index into `entries` of an item, never of a separator.
    selected: usize,
    /// The screen cell the menu hangs from: the row it was opened on. The menu
    /// opens just below it, or just above when there is no room below.
    anchor: Position,
    /// What the menu was opened on.
    target: PathBuf,
    /// Where it was last placed on screen; empty until the first draw.
    area: Rect,
}

impl Menu {
    pub(crate) fn new(entries: Vec<Entry>, anchor: Position, target: PathBuf) -> Self {
        let selected = entries
            .iter()
            .position(|entry| matches!(entry, Entry::Item { .. }))
            .unwrap_or(0);
        Self {
            entries,
            selected,
            anchor,
            target,
            area: Rect::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn entries(&self) -> &[Entry] {
        &self.entries
    }

    #[cfg(test)]
    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    pub(crate) fn target(&self) -> &Path {
        &self.target
    }

    /// The action of the selected item.
    pub(crate) fn selected_action(&self) -> Option<&Action> {
        match self.entries.get(self.selected)? {
            Entry::Item { action, .. } => Some(action),
            Entry::Separator => None,
        }
    }

    pub(crate) fn select_previous(&mut self) {
        let before = &self.entries[..self.selected];
        if let Some(index) = before.iter().rposition(is_item) {
            self.selected = index;
        }
    }

    pub(crate) fn select_next(&mut self) {
        let after = self.entries.iter().skip(self.selected + 1);
        if let Some(offset) = after.into_iter().position(is_item) {
            self.selected += 1 + offset;
        }
    }

    /// Works out where the menu goes on a screen of this size. Done before
    /// drawing, so that clicks are tested against what is on screen.
    pub(crate) fn place(&mut self, screen: Rect) {
        self.area = self.area_on(screen);
    }

    /// Whether a screen cell is inside the menu, frame included.
    pub(crate) fn contains(&self, position: Position) -> bool {
        self.area.contains(position)
    }

    /// The action of the item at a screen cell; `None` on the frame, on a
    /// separator, and outside.
    pub(crate) fn action_at(&self, position: Position) -> Option<&Action> {
        let inner = Block::bordered().inner(self.area);
        if !inner.contains(position) {
            return None;
        }
        match self.entries.get(usize::from(position.y - inner.y))? {
            Entry::Item { action, .. } => Some(action),
            Entry::Separator => None,
        }
    }

    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        let area = self.area;
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border.focused"));

        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);

        let width = usize::from(inner.width);
        let lines: Vec<Line> = self
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| match entry {
                Entry::Item { label, .. } => {
                    let text = format!(" {label:<pad$}", pad = width.saturating_sub(1));
                    if index == self.selected {
                        Line::styled(text, theme.style("list.selected"))
                    } else {
                        Line::from(text)
                    }
                }
                Entry::Separator => Line::styled("─".repeat(width), theme.style("pane.border")),
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), inner);
    }

    /// Where the menu is drawn on a screen of the given size: below the
    /// anchor, above it when it would run off the bottom, and always inside.
    fn area_on(&self, screen: Rect) -> Rect {
        let widest = self
            .entries
            .iter()
            .map(|entry| match entry {
                Entry::Item { label, .. } => label.width(),
                Entry::Separator => 0,
            })
            .max()
            .unwrap_or(0);
        // A border and a space of padding on each side.
        let width = (widest as u16 + 4).min(screen.width);
        let height = (self.entries.len() as u16 + 2).min(screen.height);
        let x = self
            .anchor
            .x
            .min(screen.right().saturating_sub(width))
            .max(screen.x);
        let below = self.anchor.y + 1;
        let y = if below + height <= screen.bottom() {
            below
        } else {
            self.anchor
                .y
                .saturating_sub(height)
                .max(screen.y)
                .min(screen.bottom().saturating_sub(height))
        };
        Rect {
            x,
            y,
            width,
            height,
        }
    }
}

fn is_item(entry: &Entry) -> bool {
    matches!(entry, Entry::Item { .. })
}
