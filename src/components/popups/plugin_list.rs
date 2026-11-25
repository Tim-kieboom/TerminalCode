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

use crate::components::popups::plain_key;
use crate::plugins::search::{Query, Subject, tags_to_offer};
use crate::plugins::tags::{Kind, Tag};
use crate::ui::backdrop;
use crate::ui::theme::Theme;

/// The keys under the list, most useful first; what does not fit is left out.
const HELP: [(&str, &str); 11] = [
    ("enter", "do it"),
    ("i", "install"),
    ("/", "search"),
    ("I", "update all"),
    ("d", "about"),
    ("k", "key"),
    ("x", "remove"),
    ("space", "on/off"),
    ("p", "place"),
    ("r", "refresh"),
    ("esc", "close"),
];
/// The keys under the list while the search box has the keyboard.
const SEARCH_HELP: [(&str, &str); 3] = [("enter", "keep"), ("esc", "clear"), ("up/down", "choose")];
/// The lines the known tags may take under the box.
const TAG_LINES: u16 = 2;
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
    /// Open the search box.
    Search,
    Cancel,
}

impl Command {
    pub(crate) fn from_key(key: KeyEvent) -> Option<Self> {
        let code = plain_key(key)?;
        // Shift and `i`: the terminal says `I`, or `i` with shift when it can.
        if key.modifiers.contains(KeyModifiers::SHIFT)
            && matches!(key.code, KeyCode::Char('i' | 'I'))
        {
            return Some(Self::UpdateAll);
        }
        Some(match code {
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
            KeyCode::Char('/') => Self::Search,
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
    /// What to find it by, after the state.
    pub tags: Vec<Tag>,
    /// Where it comes from, at the right, when that is not the official list.
    pub source: Option<String>,
    /// What it is, or what it lacks, under the list while it is chosen.
    pub detail: String,
    /// What the index says it is, which the search reads.
    pub description: String,
    pub mark: Mark,
    /// Shown dim: it cannot be used here.
    pub grey: bool,
}

impl Row {
    fn subject(&self) -> Subject<'_> {
        Subject {
            id: &self.id,
            name: &self.name,
            description: &self.description,
            tags: &self.tags,
        }
    }
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
    /// Every plugin; `rows` are the ones the search lets through.
    all: Vec<Row>,
    rows: Vec<Row>,
    selected: usize,
    /// What was typed in the search box.
    query: String,
    /// The search box has the keyboard.
    typing: bool,
    /// What the index is: from this version, cached, fetched, or why it is not.
    note: Option<Note>,
    /// What is being downloaded now, when something is.
    busy: Option<String>,
}

impl PluginList {
    pub(crate) fn new(rows: Vec<Row>, note: Option<Note>) -> Self {
        Self {
            all: rows.clone(),
            rows,
            selected: 0,
            query: String::new(),
            typing: false,
            note,
            busy: None,
        }
    }

    /// New rows; the plugin that was chosen stays chosen when it is still there, and the search
    /// is still applied.
    pub(crate) fn set_rows(&mut self, rows: Vec<Row>) {
        self.all = rows;
        self.refilter();
    }

    /// Takes the rows the search lets through out of `all`, keeping the plugin chosen.
    fn refilter(&mut self) {
        let chosen = self.selected_id().map(Box::<str>::from);
        let query = Query::parse(&self.query);
        self.rows = self
            .all
            .iter()
            .filter(|row| query.matches(row.subject()))
            .cloned()
            .collect();
        self.selected = chosen
            .and_then(|id| self.rows.iter().position(|row| row.id == id))
            .unwrap_or(0)
            .min(self.rows.len().saturating_sub(1));
    }

    /// Opens the search box: what was typed before is still in it.
    pub(crate) fn start_search(&mut self) {
        self.typing = true;
    }

    /// Whether the search box has the keyboard.
    pub(crate) fn is_searching(&self) -> bool {
        self.typing
    }

    /// Whether the list is limited by a search.
    pub(crate) fn has_query(&self) -> bool {
        !self.query.is_empty()
    }

    /// Forgets the search and shows every plugin.
    pub(crate) fn clear_search(&mut self) {
        self.query.clear();
        self.typing = false;
        self.refilter();
    }

    /// A key while the box has the keyboard: it types, or one of `enter` (keep what was
    /// typed and give the keyboard to the list), `esc` (forget it), and the arrows, which
    /// choose in the list.
    pub(crate) fn search_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        let held = KeyModifiers::CONTROL | KeyModifiers::ALT;
        match key.code {
            KeyCode::Esc => self.clear_search(),
            KeyCode::Enter => self.typing = false,
            KeyCode::Up => self.step(false),
            KeyCode::Down => self.step(true),
            KeyCode::Backspace => {
                self.query.pop();
                self.refilter();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.query.clear();
                self.refilter();
            }
            KeyCode::Char(typed) if !key.modifiers.intersects(held) => {
                self.query.push(typed);
                self.refilter();
            }
            _ => {}
        }
    }

    /// What to say under the box: a tag that does not exist, or what the editor has of a
    /// language that was asked for. A tag that is still being typed is not wrong yet: the tags
    /// the box lists are the answer.
    fn message(&self) -> Option<Note> {
        let query = Query::parse(&self.query);
        if let [first, ..] = query.unknown_tags() {
            let being_typed = self.typing && !tags_to_offer(&self.query).is_empty();
            return (!being_typed).then(|| Note {
                text: format!("no tag `{first}`: press / to see the tags"),
                warn: true,
            });
        }
        let text = query.language_note(self.rows.is_empty())?;
        Some(Note { text, warn: false })
    }

    /// The lines above the list that belong to the search: the box, the tags to choose from
    /// while it is typed in, and one for what is to be said. None without a search.
    fn header_height(&self) -> u16 {
        match (self.typing, self.query.is_empty()) {
            (false, true) => 0,
            (true, _) => 2 + TAG_LINES,
            (false, false) => 2,
        }
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

    /// The box, centered on `screen`.
    fn area(&self, screen: Rect) -> Rect {
        let wanted = self
            .all
            .iter()
            .map(|row| {
                4 + row_text_width(row)
                    + row.state.width()
                    + tags_width(row)
                    + source_width(row)
                    + 6
            })
            .max()
            .unwrap_or(0)
            .max(help_width(&HELP, HELP.len()) + 4);
        let width = (wanted as u16).clamp(NARROWEST, WIDEST).min(screen.width);
        let header = self.header_height();
        let height = (self.all.len() as u16 + 8 + header)
            .max(12 + header)
            .min(screen.height);
        Rect {
            x: screen.x + (screen.width - width) / 2,
            y: screen.y + (screen.height - height) / 2,
            width,
            height,
        }
    }

    /// The rows of the list: the box inside its frame, less the five lines under them.
    fn rows_area(&self, screen: Rect) -> Rect {
        let inner = Block::new().borders(Borders::ALL).inner(self.area(screen));
        let footer = 5.min(inner.height);
        let header = self.header_height().min(inner.height - footer);
        Rect {
            y: inner.y + header,
            height: inner.height - footer - header,
            ..inner
        }
    }

    /// Whether a screen cell is inside the box.
    pub(crate) fn contains(&self, screen: Rect, column: u16, row: u16) -> bool {
        self.area(screen)
            .contains(ratatui::layout::Position::new(column, row))
    }

    /// The plugin under a screen cell, as an index into the rows.
    pub(crate) fn row_at(&self, screen: Rect, column: u16, row: u16) -> Option<usize> {
        let list = self.rows_area(screen);
        if !list.contains(ratatui::layout::Position::new(column, row)) {
            return None;
        }
        let top = crate::components::list_cursor::first_when_drawn(
            self.selected,
            usize::from(list.height),
        );
        let index = top + usize::from(row - list.y);
        (index < self.rows.len()).then_some(index)
    }

    /// Selects the row at `index`, if there is one.
    pub(crate) fn select_row(&mut self, index: usize) {
        if index < self.rows.len() {
            self.selected = index;
        }
    }

    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        let screen = frame.area();
        let area = self.area(screen);
        let border = theme.style("pane.border.focused");
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(border)
            .title(Line::from(vec![
                Span::styled(" Plugins ", theme.style("pane.title")),
                Span::styled(self.count_text(), theme.style("tab.inactive")),
            ]));
        let inner = block.inner(area);
        backdrop::clear(frame, theme, area);
        frame.render_widget(block, area);

        // The list, a rule, the detail (two lines), the status and the keys.
        let footer = 5.min(inner.height);
        let header = self.header_height().min(inner.height - footer);
        let rows_height = inner.height - footer - header;
        let list = Rect {
            y: inner.y + header,
            height: rows_height,
            ..inner
        };
        let at = |offset: u16, height: u16| Rect {
            y: inner.y + header + rows_height + offset,
            height,
            ..inner
        };
        let search = Rect {
            height: header,
            ..inner
        };
        self.render_search(frame, theme, search);
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
        let lines = match (lines.is_empty(), self.query.is_empty()) {
            (true, true) => vec![Line::styled(
                " No plugins yet. Press r to ask the list again.",
                theme.style("tab.inactive"),
            )],
            (true, false) => vec![Line::styled(
                " No plugin matches. Esc clears the search.",
                theme.style("tab.inactive"),
            )],
            (false, _) => lines,
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
            Paragraph::new(help_line(usize::from(help.width), theme, self.help_table())),
            help,
        );
    }

    /// How many plugins the title says: those shown, and of how many when a search limits it.
    fn count_text(&self) -> String {
        match self.query.is_empty() {
            true => format!("{} ", self.rows.len()),
            false => format!("{}/{} ", self.rows.len(), self.all.len()),
        }
    }

    fn help_table(&self) -> &'static [(&'static str, &'static str)] {
        match self.typing {
            true => &SEARCH_HELP,
            false => &HELP,
        }
    }

    /// The search box, the tags to choose from and what there is to say, in `area`.
    fn render_search(&self, frame: &mut Frame, theme: &Theme, area: Rect) {
        if area.height == 0 {
            return;
        }
        let faint = theme.style("tab.inactive");
        let text = theme.style("explorer.file");
        let line = match self.typing {
            true => Line::from(vec![
                Span::styled(" / ", faint),
                Span::styled(self.query.clone(), text),
                Span::styled("▏", text),
            ]),
            false => Line::from(vec![
                Span::styled(" / ", faint),
                Span::styled(self.query.clone(), text),
                Span::styled("   esc clears", faint),
            ]),
        };
        let row = |offset: u16, height: u16| Rect {
            y: area.y + offset,
            height: height.min(area.height.saturating_sub(offset)),
            ..area
        };
        frame.render_widget(Paragraph::new(line), row(0, 1));
        let mut next = 1;
        if self.typing {
            let offered = tags_to_offer(&self.query);
            let tags = match offered.is_empty() {
                true => " no tag starts with that".to_owned(),
                false => format!(" tags: {}", tags_offered(&offered)),
            };
            frame.render_widget(
                Paragraph::new(Line::styled(tags, faint)).wrap(Wrap { trim: false }),
                row(next, TAG_LINES),
            );
            next += TAG_LINES;
        }
        if let Some(message) = self.message() {
            let slot = match message.warn {
                true => "git.modified",
                false => "notification.info",
            };
            frame.render_widget(
                Paragraph::new(Line::styled(
                    format!(" {}", message.text),
                    theme.style(slot),
                )),
                row(next, 1),
            );
        }
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
        if !row.tags.is_empty() {
            spans.push(Span::styled(format!("  {}", tags_text(&row.tags)), faint));
        }
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

/// The tags as they are drawn: `#git #rust`.
fn tags_text(tags: &[Tag]) -> String {
    tags.iter()
        .map(|tag| format!("#{}", tag.name()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn tags_width(row: &Row) -> usize {
    match row.tags.is_empty() {
        true => 0,
        false => 2 + tags_text(&row.tags).width(),
    }
}

fn source_width(row: &Row) -> usize {
    row.source.as_ref().map_or(0, |source| source.width() + 2)
}

/// The tags as the box lists them: what a plugin is, then the languages, a dot between.
fn tags_offered(tags: &[Tag]) -> String {
    let mut text = String::new();
    let mut last: Option<Kind> = None;
    for tag in tags {
        match last {
            Some(kind) if kind != tag.kind() => text.push_str(" · "),
            Some(_) => text.push(' '),
            None => {}
        }
        text.push_str(tag.name());
        last = Some(tag.kind());
    }
    text
}

/// The width of the first `count` keys of `table`, with their words.
fn help_width(table: &[(&str, &str)], count: usize) -> usize {
    table
        .iter()
        .take(count)
        .map(|(key, what)| key.width() + 1 + what.width())
        .sum::<usize>()
        + 3 * count.saturating_sub(1)
}

/// The keys under the list: the key bold, what it does dim, as many as fit.
fn help_line(width: usize, theme: &Theme, table: &[(&'static str, &'static str)]) -> Line<'static> {
    let fit = (1..=table.len())
        .rev()
        .find(|&count| help_width(table, count) < width)
        .unwrap_or(1);
    let key = theme.style("explorer.file").add_modifier(Modifier::BOLD);
    let what = theme.style("tab.inactive");
    let mut spans = vec![Span::raw(" ")];
    for (n, (keys, does)) in table.iter().take(fit).enumerate() {
        if n > 0 {
            spans.push(Span::styled(" · ", what));
        }
        spans.push(Span::styled(*keys, key));
        spans.push(Span::styled(format!(" {does}"), what));
    }
    Line::from(spans)
}
