//! The plugin manager: the plugins of the index and the ones the user made, in one
//! list with their state. Up and down choose, Enter does the likeliest thing (install,
//! update, turn on, place), and a letter does one thing: `i` installs or updates, `I` updates all, `d` reads about it, `k` chooses the key that opens it, `x`
//! removes, `space` turns on or off, `p` places, `r` asks the index again, Esc closes
//! (while something is downloading it stops that instead).

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::ui::backdrop;
use crate::ui::theme::Theme;

/// The keys under the list, most useful first; what does not fit is left out.
const HELP: [(&str, &str); 10] = [
    ("enter", "do it"),
    ("i", "install"),
    ("I", "update all"),
    ("d", "about"),
    ("k", "key"),
    ("x", "remove"),
    ("space", "on/off"),
    ("p", "place"),
    ("r", "refresh"),
    ("esc", "close"),
];
const NARROWEST: u16 = 50;
const WIDEST: u16 = 120;

/// What a key press asks the list to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Previous,
    Next,
    /// The likeliest thing for the plugin chosen.
    Primary,
    Install,
    /// Update every plugin that has a newer version.
    UpdateAll,
    /// Read what the plugin says about itself.
    Describe,
    /// Choose the key that opens the plugin.
    BindKey,
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
        // Shift and `i`: the terminal says `I`, or `i` with shift when it can.
        if key.modifiers.contains(KeyModifiers::SHIFT)
            && matches!(key.code, KeyCode::Char('i' | 'I'))
        {
            return Some(Self::UpdateAll);
        }
        Some(match key.code {
            KeyCode::Up => Self::Previous,
            KeyCode::Down => Self::Next,
            KeyCode::Enter => Self::Primary,
            KeyCode::Char('I') => Self::UpdateAll,
            KeyCode::Char('i' | 'u') => Self::Install,
            KeyCode::Char('d') => Self::Describe,
            KeyCode::Char('k') => Self::BindKey,
            KeyCode::Char('x') | KeyCode::Delete => Self::Remove,
            KeyCode::Char(' ') => Self::Toggle,
            KeyCode::Char('p') => Self::Place,
            KeyCode::Char('r') => Self::Refresh,
            KeyCode::Esc => Self::Cancel,
            _ => return None,
        })
    }
}

/// What a plugin is, as the symbol in front of its row says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Mark {
    /// In the index, not installed: `+`.
    #[default]
    NotInstalled,
    /// Installed and running: `●`.
    On,
    /// Installed and off: `○`.
    Off,
    /// Installed and a newer version can be installed: `↑`.
    Update,
    /// In the index but this editor cannot run it: `–`.
    Unavailable,
    /// It cannot run, or it failed too often: `✕`.
    Broken,
}

impl Mark {
    fn symbol(self) -> &'static str {
        match self {
            Self::NotInstalled => "+",
            Self::On => "●",
            Self::Off => "○",
            Self::Update => "↑",
            Self::Unavailable => "–",
            Self::Broken => "✕",
        }
    }

    /// The theme slot the symbol is drawn in.
    fn slot(self) -> &'static str {
        match self {
            Self::NotInstalled => "git.untracked",
            Self::On => "git.added",
            Self::Off | Self::Unavailable => "tab.inactive",
            Self::Update => "git.modified",
            Self::Broken => "git.deleted",
        }
    }
}

/// One plugin as the list shows it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Row {
    pub id: Box<str>,
    pub name: String,
    pub version: String,
    /// Its state, after the name.
    pub state: String,
    /// Where it comes from, at the right, when that is not the official list.
    pub source: Option<String>,
    /// What it is, or what it lacks, under the list while it is chosen.
    pub detail: String,
    pub mark: Mark,
    /// Shown dim: it cannot be used here.
    pub grey: bool,
}

/// What the list says about the index, under the plugins.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Note {
    pub text: String,
    /// Something is wrong (offline, a source that failed): drawn to be noticed.
    pub warn: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct PluginList {
    rows: Vec<Row>,
    selected: usize,
    /// What the index is: from this version, cached, fetched, or why it is not.
    note: Option<Note>,
    /// What is being downloaded now, when something is.
    busy: Option<String>,
}

impl PluginList {
    pub(crate) fn new(rows: Vec<Row>, note: Option<Note>) -> Self {
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

    pub(crate) fn set_note(&mut self, note: Option<Note>) {
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
            .map(|row| 4 + row_text_width(row) + row.state.width() + source_width(row) + 6)
            .max()
            .unwrap_or(0)
            .max(help_width(HELP.len()) + 4);
        let width = (wanted as u16).clamp(NARROWEST, WIDEST).min(screen.width);
        let height = (self.rows.len() as u16 + 8).max(12).min(screen.height);
        let area = Rect {
            x: screen.x + (screen.width - width) / 2,
            y: screen.y + (screen.height - height) / 2,
            width,
            height,
        };
        let border = theme.style("pane.border.focused");
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border)
            .title(Line::from(vec![
                Span::styled(" Plugins ", theme.style("pane.title")),
                Span::styled(format!("{} ", self.rows.len()), theme.style("tab.inactive")),
            ]));
        let inner = block.inner(area);
        backdrop::clear(frame, theme, area);
        frame.render_widget(block, area);

        // The list, a rule, the detail (two lines), the status and the keys.
        let footer = 5.min(inner.height);
        let rows_height = inner.height - footer;
        let list = Rect {
            height: rows_height,
            ..inner
        };
        let at = |offset: u16, height: u16| Rect {
            y: inner.y + rows_height + offset,
            height,
            ..inner
        };
        let rule = at(0, 1.min(footer));
        let detail = at(1, 2.min(footer.saturating_sub(1)));
        let status = at(3, 1.min(footer.saturating_sub(3)));
        let help = at(4, 1.min(footer.saturating_sub(4)));

        let name_width = self.rows.iter().map(row_text_width).max().unwrap_or(0);
        let top = self
            .selected
            .saturating_sub(usize::from(rows_height).saturating_sub(1));
        let shown = usize::from(rows_height);
        let lines: Vec<Line> = self
            .rows
            .iter()
            .enumerate()
            .skip(top)
            .take(shown)
            .map(|(at, row)| {
                self.line_of(
                    row,
                    at == self.selected,
                    name_width,
                    usize::from(list.width),
                    theme,
                )
            })
            .collect();
        let lines = match lines.is_empty() {
            true => vec![Line::styled(
                " No plugins yet. Press r to ask the list again.",
                theme.style("tab.inactive"),
            )],
            false => lines,
        };
        frame.render_widget(Paragraph::new(lines), list);
        // There are more rows above or below what is shown.
        let mut more = |glyph: &'static str, y: u16| {
            frame.render_widget(
                Paragraph::new(Line::styled(glyph, theme.style("tab.inactive")))
                    .alignment(ratatui::layout::Alignment::Right),
                Rect {
                    y,
                    height: 1,
                    ..list
                },
            );
        };
        if top > 0 && rows_height > 0 {
            more("▲", list.y);
        }
        if top + shown < self.rows.len() && rows_height > 0 {
            more("▼", list.y + rows_height - 1);
        }

        // The rule joins the left and right borders.
        let across = Rect {
            x: area.x,
            width: area.width,
            ..rule
        };
        let bar = format!(
            "├{}┤",
            "─".repeat(usize::from(area.width).saturating_sub(2))
        );
        frame.render_widget(Paragraph::new(Line::styled(bar, border)), across);
        let about = self
            .rows
            .get(self.selected)
            .map(|row| row.detail.as_str())
            .unwrap_or_default();
        frame.render_widget(
            Paragraph::new(Line::styled(
                format!(" {about}"),
                theme.style("explorer.hint"),
            ))
            .wrap(Wrap { trim: false }),
            detail,
        );

        let line = match (&self.busy, &self.note) {
            (Some(busy), _) => Line::styled(
                format!(" {busy} (esc stops)"),
                theme.style("notification.info"),
            ),
            (None, Some(note)) => {
                let slot = match note.warn {
                    true => "git.modified",
                    false => "tab.inactive",
                };
                Line::styled(format!(" {}", note.text), theme.style(slot))
            }
            (None, None) => Line::default(),
        };
        frame.render_widget(Paragraph::new(line), status);
        frame.render_widget(
            Paragraph::new(help_line(usize::from(help.width), theme)),
            help,
        );
    }

    /// One row: the symbol, the name and version, the state and, at the right, the source.
    fn line_of(
        &self,
        row: &Row,
        selected: bool,
        name_width: usize,
        width: usize,
        theme: &Theme,
    ) -> Line<'static> {
        let base = match (selected, row.grey) {
            (true, _) => theme.style("list.selected"),
            (false, true) => theme.style("tab.inactive"),
            (false, false) => theme.style("explorer.file"),
        };
        // The symbol keeps its own color, except on the selected row, which is one color.
        let symbol = match selected {
            true => base,
            false => theme.style(row.mark.slot()),
        };
        let faint = match selected {
            true => base,
            false => theme.style("tab.inactive"),
        };
        let name_pad = name_width - row_text_width(row);
        let mut spans = vec![
            Span::styled(format!(" {} ", row.mark.symbol()), symbol),
            Span::styled(row.name.clone(), base),
        ];
        if !row.version.is_empty() {
            spans.push(Span::styled(format!(" {}", row.version), faint));
        }
        spans.push(Span::styled(format!("{}  ", " ".repeat(name_pad)), base));
        spans.push(Span::styled(row.state.clone(), base));
        if let Some(source) = &row.source {
            let used: usize = spans.iter().map(|span| span.content.width()).sum();
            let room = width.saturating_sub(used + source.width() + 1);
            spans.push(Span::styled(" ".repeat(room.max(2)), base));
            spans.push(Span::styled(source.clone(), faint));
        }
        Line::from(spans).style(base)
    }
}

/// The width of the name and the version, as they are drawn.
fn row_text_width(row: &Row) -> usize {
    row.name.width()
        + match row.version.is_empty() {
            true => 0,
            false => 1 + row.version.width(),
        }
}

fn source_width(row: &Row) -> usize {
    row.source.as_ref().map_or(0, |source| source.width() + 2)
}

/// The width of the first `count` keys of [`HELP`], with their words.
fn help_width(count: usize) -> usize {
    HELP.iter()
        .take(count)
        .map(|(key, what)| key.width() + 1 + what.width())
        .sum::<usize>()
        + 3 * count.saturating_sub(1)
}

/// The keys under the list: the key bold, what it does dim, as many as fit.
fn help_line(width: usize, theme: &Theme) -> Line<'static> {
    let fit = (1..=HELP.len())
        .rev()
        .find(|&count| help_width(count) < width)
        .unwrap_or(1);
    let key = theme.style("explorer.file").add_modifier(Modifier::BOLD);
    let what = theme.style("tab.inactive");
    let mut spans = vec![Span::raw(" ")];
    for (n, (keys, does)) in HELP.iter().take(fit).enumerate() {
        if n > 0 {
            spans.push(Span::styled(" · ", what));
        }
        spans.push(Span::styled(*keys, key));
        spans.push(Span::styled(format!(" {does}"), what));
    }
    Line::from(spans)
}
