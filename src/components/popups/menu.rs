//! A context menu: a short list of actions in a small box beside what it is
//! about. Up and down move over the items, Enter runs one, Esc closes it.
//!
//! The menu does not know what its actions do. It remembers the path it was
//! opened on, so they act on that and not on whatever is selected later.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::event::action::Action;
use crate::ui::theme::Theme;

/// The least space between a label and its keys.
const KEY_GAP: usize = 2;
/// Marks an item that opens a menu of its own.
const GROUP_MARK: &str = "▸";

/// One line of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Entry {
    Item {
        label: Cow<'static, str>,
        action: Action,
        /// The keys bound to the action, shown at the right edge.
        keys: Option<String>,
    },
    /// An item that opens a menu of its own to the right, with the entries it holds.
    Group {
        label: Cow<'static, str>,
        entries: Vec<Entry>,
    },
    /// A rule between two groups of items; the selection skips it.
    Separator,
}

/// What a key press asks the menu to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Previous,
    Next,
    /// Into the menu of a group.
    Right,
    /// Out of the menu of a group.
    Left,
    Run,
    Cancel,
}

/// What a key or a click did to the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Step {
    /// The menu stays open.
    Stay,
    /// The menu is done with nothing run.
    Close,
    /// An item was chosen.
    Run(Action),
    /// A click that was not on the menu.
    Outside,
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
            KeyCode::Right => Self::Right,
            KeyCode::Left => Self::Left,
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
    /// The file or folder the menu was opened on; `None` for a menu on the
    /// editor, whose actions work on what the editor has selected.
    target: Option<PathBuf>,
    /// Where it was last placed on screen; empty until the first draw.
    area: Rect,
    /// The menu of the selected group, while it is open. It has the keyboard.
    submenu: Option<Box<Menu>>,
    /// The menu belongs to the app and not to what was clicked: the keyboard stays
    /// where it was when the menu closes.
    keeps_focus: bool,
    /// The menu starts on the anchor's row and not under it.
    from_anchor_row: bool,
}

impl Menu {
    pub(crate) fn new(entries: Vec<Entry>, anchor: Position, target: PathBuf) -> Self {
        Self::with_target(entries, anchor, Some(target))
    }

    /// The main menu: not about a file or the text, so what runs from it leaves the
    /// keyboard where it was.
    pub(crate) fn on_app(entries: Vec<Entry>, anchor: Position) -> Self {
        let mut menu = Self::with_target(entries, anchor, None);
        menu.keeps_focus = true;
        menu
    }

    /// The menu starts on the row of its anchor, not under it: for a menu that has
    /// no row of its own to hang from.
    pub(crate) fn starting_on_anchor_row(mut self) -> Self {
        self.from_anchor_row = true;
        self
    }

    /// Whether the keyboard stays where it was when the menu closes.
    pub(crate) fn keeps_focus(&self) -> bool {
        self.keeps_focus
    }

    /// A menu on the editor text rather than on a path.
    pub(crate) fn on_editor(entries: Vec<Entry>, anchor: Position) -> Self {
        Self::with_target(entries, anchor, None)
    }

    fn with_target(entries: Vec<Entry>, anchor: Position, target: Option<PathBuf>) -> Self {
        let selected = entries.iter().position(is_item).unwrap_or(0);
        Self {
            entries,
            selected,
            anchor,
            target,
            area: Rect::default(),
            submenu: None,
            keeps_focus: false,
            from_anchor_row: false,
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

    /// The labels of the items, for tests.
    #[cfg(test)]
    pub(crate) fn labels_for_test(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter_map(|entry| match entry {
                Entry::Item { label, .. } | Entry::Group { label, .. } => Some(label.as_ref()),
                Entry::Separator => None,
            })
            .collect()
    }

    pub(crate) fn target(&self) -> Option<&Path> {
        self.target.as_deref()
    }

    /// The action of the selected item.
    #[cfg(test)]
    pub(crate) fn selected_action(&self) -> Option<&Action> {
        match self.entries.get(self.selected)? {
            Entry::Item { action, .. } => Some(action),
            Entry::Group { .. } | Entry::Separator => None,
        }
    }

    /// The menu of the open group, for tests.
    #[cfg(test)]
    pub(crate) fn submenu(&self) -> Option<&Menu> {
        self.submenu.as_deref()
    }

    /// Does what `command` asks. The open submenu, if there is one, has the keyboard.
    pub(crate) fn key(&mut self, command: Command) -> Step {
        if let Some(sub) = self.submenu.as_mut() {
            return match command {
                Command::Left | Command::Cancel => {
                    self.submenu = None;
                    Step::Stay
                }
                other => sub.key(other),
            };
        }
        match command {
            Command::Previous => self.select_previous(),
            Command::Next => self.select_next(),
            Command::Cancel => return Step::Close,
            Command::Left => {}
            Command::Right => self.open_group(),
            Command::Run => return self.run_selected(),
        }
        Step::Stay
    }

    fn run_selected(&mut self) -> Step {
        match self.entries.get(self.selected) {
            Some(Entry::Item { action, .. }) => Step::Run(action.clone()),
            Some(Entry::Group { .. }) => {
                self.open_group();
                Step::Stay
            }
            _ => Step::Stay,
        }
    }

    /// Opens the menu of the selected group, if the selection is on one.
    fn open_group(&mut self) {
        self.open_group_at(self.selected);
    }

    fn open_group_at(&mut self, index: usize) {
        let Some(Entry::Group { entries, .. }) = self.entries.get(index) else {
            return;
        };
        self.selected = index;
        self.submenu = Some(Box::new(Self::with_target(
            entries.clone(),
            self.anchor,
            None,
        )));
    }

    /// A left click at a screen cell: runs the item there, opens the group there,
    /// or tells that it was not on the menu at all.
    pub(crate) fn click(&mut self, position: Position) -> Step {
        if let Some(sub) = self.submenu.as_mut()
            && sub.contains(position)
        {
            return sub.click(position);
        }
        let inner = Block::bordered().inner(self.area);
        if !self.area.contains(position) {
            return Step::Outside;
        }
        if !inner.contains(position) {
            return Step::Stay;
        }
        let index = usize::from(position.y - inner.y);
        match self.entries.get(index) {
            Some(Entry::Item { action, .. }) => Step::Run(action.clone()),
            Some(Entry::Group { .. }) => {
                self.open_group_at(index);
                Step::Stay
            }
            _ => Step::Stay,
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
        let (area, selected) = (self.area, self.selected);
        if let Some(sub) = self.submenu.as_mut() {
            // Its first item is level with the group, to the right of this menu.
            let row = area.y + 1 + selected as u16;
            sub.anchor = Position::new(area.right(), row.saturating_sub(2));
            sub.place(screen);
        }
    }

    /// Whether a screen cell is inside the menu or its open submenu, frame included.
    pub(crate) fn contains(&self, position: Position) -> bool {
        self.area.contains(position)
            || self
                .submenu
                .as_ref()
                .is_some_and(|sub| sub.contains(position))
    }

    /// The action of the item at a screen cell; `None` on the frame, on a
    /// separator, on a group and outside.
    #[cfg(test)]
    pub(crate) fn action_at(&self, position: Position) -> Option<&Action> {
        if let Some(sub) = &self.submenu
            && sub.contains(position)
        {
            return sub.action_at(position);
        }
        let inner = Block::bordered().inner(self.area);
        if !inner.contains(position) {
            return None;
        }
        match self.entries.get(usize::from(position.y - inner.y))? {
            Entry::Item { action, .. } => Some(action),
            Entry::Group { .. } | Entry::Separator => None,
        }
    }

    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        let area = self.area;
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border.focused"));

        let inner = block.inner(area);
        crate::ui::backdrop::clear(frame, theme, area);
        frame.render_widget(block, area);

        let width = usize::from(inner.width);
        let lines: Vec<Line> = self
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| match entry {
                Entry::Item { label, keys, .. } => {
                    let keys = keys.as_deref().unwrap_or("");
                    let gap = width.saturating_sub(label.width() + keys.width() + 2);
                    let text = format!(" {label}{}{keys} ", " ".repeat(gap));
                    if index == self.selected {
                        Line::styled(text, theme.style("list.selected"))
                    } else {
                        Line::from(text)
                    }
                }
                Entry::Group { label, .. } => {
                    let gap = width.saturating_sub(label.width() + GROUP_MARK.width() + 2);
                    let text = format!(" {label}{}{GROUP_MARK} ", " ".repeat(gap));
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
        if let Some(sub) = &self.submenu {
            sub.render(frame, theme);
        }
    }

    /// Where the menu is drawn on a screen of the given size: below the
    /// anchor, above it when it would run off the bottom, and always inside.
    fn area_on(&self, screen: Rect) -> Rect {
        let widest = self
            .entries
            .iter()
            .map(|entry| match entry {
                Entry::Item { label, keys, .. } => {
                    label.width() + keys.as_ref().map_or(0, |keys| KEY_GAP + keys.width())
                }
                Entry::Group { label, .. } => label.width() + KEY_GAP + GROUP_MARK.width(),
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
        let below = self.anchor.y + u16::from(!self.from_anchor_row);
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
    matches!(entry, Entry::Item { .. } | Entry::Group { .. })
}
