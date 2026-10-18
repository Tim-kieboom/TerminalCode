//! The plugin manager: the plugins of the index and the ones the user made, in one
//! list with their state. Up and down choose, Enter does the likeliest thing (install,
//! update, turn on, place), and a letter does one thing: `i` installs or updates, `x`
//! removes, `space` turns on or off, `p` places, `r` asks the index again, Esc closes
//! (while something is downloading it stops that instead).

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::ui::backdrop;
use crate::ui::theme::Theme;

const HELP: &str =
    "enter: do it | i: install/update | x: remove | space: on/off | p: place | r: refresh | esc";
const NARROWEST: u16 = 50;
const WIDEST: u16 = 110;

/// What a key press asks the list to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Previous,
    Next,
    /// The likeliest thing for the plugin chosen.
    Primary,
    Install,
    Remove,
    Toggle,
    Place,
    Refresh,
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
            KeyCode::Up | KeyCode::Char('k') => Self::Previous,
            KeyCode::Down | KeyCode::Char('j') => Self::Next,
            KeyCode::Enter => Self::Primary,
            KeyCode::Char('i' | 'u') => Self::Install,
            KeyCode::Char('x') | KeyCode::Delete => Self::Remove,
            KeyCode::Char(' ') => Self::Toggle,
            KeyCode::Char('p') => Self::Place,
            KeyCode::Char('r') => Self::Refresh,
            KeyCode::Esc => Self::Cancel,
            _ => return None,
        })
    }
}

/// One plugin as the list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    pub id: Box<str>,
    /// Its name and version.
    pub name: String,
    /// Its state, at the right of the name.
    pub tag: String,
    /// What it is, or what it lacks, under the list while it is chosen.
    pub detail: String,
    /// Shown dim: it cannot be used here.
    pub grey: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PluginList {
    rows: Vec<Row>,
    selected: usize,
    /// What the index is: from this version, cached, fetched, or why it is not.
    note: Option<String>,
    /// What is being downloaded now, when something is.
    busy: Option<String>,
}

impl PluginList {
    pub(crate) fn new(rows: Vec<Row>, note: Option<String>) -> Self {
        Self {
            rows,
            selected: 0,
            note,
            busy: None,
        }
    }

    /// New rows; the plugin that was chosen stays chosen when it is still there.
    pub(crate) fn set_rows(&mut self, rows: Vec<Row>) {
        let chosen = self.selected_id().map(Box::<str>::from);
        self.rows = rows;
        self.selected = chosen
            .and_then(|id| self.rows.iter().position(|row| row.id == id))
            .unwrap_or(0)
            .min(self.rows.len().saturating_sub(1));
    }

    pub(crate) fn set_note(&mut self, note: Option<String>) {
        self.note = note;
    }

    pub(crate) fn set_busy(&mut self, busy: Option<String>) {
        self.busy = busy;
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.busy.is_some()
    }

    pub(crate) fn selected_id(&self) -> Option<&str> {
        self.rows.get(self.selected).map(|row| &*row.id)
    }

    pub(crate) fn select(&mut self, id: &str) {
        if let Some(at) = self.rows.iter().position(|row| &*row.id == id) {
            self.selected = at;
        }
    }

    pub(crate) fn step(&mut self, forwards: bool) {
        let last = self.rows.len().saturating_sub(1);
        self.selected = match forwards {
            true => (self.selected + 1).min(last),
            false => self.selected.saturating_sub(1),
        };
    }

    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        let screen = frame.area();
        let wanted = self
            .rows
            .iter()
            .map(|row| row.name.width() + row.tag.width() + 6)
            .max()
            .unwrap_or(0)
            .max(HELP.width() + 4);
        let width = (wanted as u16).clamp(NARROWEST, WIDEST).min(screen.width);
        let height = (self.rows.len() as u16 + 5).max(9).min(screen.height);
        let area = Rect {
            x: screen.x + (screen.width - width) / 2,
            y: screen.y + (screen.height - height) / 2,
            width,
            height,
        };
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border.focused"))
            .title(Line::styled(" Plugins ", theme.style("pane.title")));
        let inner = block.inner(area);
        backdrop::clear(frame, theme, area);
        frame.render_widget(block, area);

        let rows_height = inner.height.saturating_sub(3);
        let list = Rect {
            height: rows_height,
            ..inner
        };
        let detail = Rect {
            y: inner.y + rows_height,
            height: inner.height.min(1),
            ..inner
        };
        let status = Rect {
            y: detail.y + 1,
            height: 1,
            ..inner
        };
        let help = Rect {
            y: detail.y + 2,
            height: 1,
            ..inner
        };

        let name_width = self
            .rows
            .iter()
            .map(|row| row.name.width())
            .max()
            .unwrap_or(0);
        let top = self
            .selected
            .saturating_sub(usize::from(rows_height).saturating_sub(1));
        let lines: Vec<Line> = self
            .rows
            .iter()
            .enumerate()
            .skip(top)
            .take(usize::from(rows_height))
            .map(|(at, row)| {
                let pad = " ".repeat(name_width - row.name.width());
                let text = format!(" {}{pad}  {}", row.name, row.tag);
                let style = match (at == self.selected, row.grey) {
                    (true, _) => theme.style("list.selected"),
                    (false, true) => theme.style("tab.inactive"),
                    (false, false) => theme.style("explorer.file"),
                };
                Line::from(Span::styled(text, style)).style(style)
            })
            .collect();
        let lines = match lines.is_empty() {
            true => vec![Line::styled(
                " No plugins in the index or in the plugins folder.",
                theme.style("tab.inactive"),
            )],
            false => lines,
        };
        frame.render_widget(Paragraph::new(lines), list);

        let about = self
            .rows
            .get(self.selected)
            .map(|row| row.detail.as_str())
            .unwrap_or_default();
        frame.render_widget(
            Paragraph::new(Line::styled(
                format!(" {about}"),
                theme.style("tab.inactive"),
            )),
            detail,
        );

        let (text, style) = match (&self.busy, &self.note) {
            (Some(busy), _) => (
                format!(" {busy} (esc stops)"),
                theme.style("notification.info"),
            ),
            (None, Some(note)) => (format!(" {note}"), theme.style("tab.inactive")),
            (None, None) => (String::new(), theme.style("tab.inactive")),
        };
        frame.render_widget(Paragraph::new(Line::styled(text, style)), status);
        frame.render_widget(
            Paragraph::new(Line::styled(
                format!(" {HELP}"),
                theme.style("tab.inactive"),
            )),
            help,
        );
    }
}
