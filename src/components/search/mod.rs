//! Project-wide text search.
//!
//! The query is literal text unless regex is switched on, and ignores case
//! unless match case is. Results stream in from a worker ([`worker`]) as one
//! row per matching line; enter opens the file at the match.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::ui::theme::Theme;

pub(crate) use hit::Hit;
use worker::SearchHandle;
pub(crate) use worker::{MAX_HITS, start as start_search};

mod hit;
#[cfg(test)]
mod tests;
mod worker;

const WIDTH: u16 = 110;
const MAX_ROWS: u16 = 16;
const PAGE: usize = 10;

/// What the query means.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Options {
    pub(crate) case_sensitive: bool,
    pub(crate) regex: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Status {
    /// Nothing to search for yet.
    Idle,
    Running,
    Done {
        files: usize,
        truncated: bool,
    },
    /// The query is not a valid pattern; the text says why.
    Invalid(String),
}

/// What a key press asks the search to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Insert(char),
    Backspace,
    ClearQuery,
    ToggleCase,
    ToggleRegex,
    Previous,
    Next,
    PageUp,
    PageDown,
    Open,
    Cancel,
}

impl Command {
    pub(crate) fn from_key(key: KeyEvent) -> Option<Self> {
        if key.kind == KeyEventKind::Release {
            return None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        Some(match (key.code, ctrl, alt) {
            (KeyCode::Esc, false, false) => Self::Cancel,
            (KeyCode::Enter, false, false) => Self::Open,
            (KeyCode::Up, false, false) => Self::Previous,
            (KeyCode::Down, false, false) => Self::Next,
            (KeyCode::PageUp, false, false) => Self::PageUp,
            (KeyCode::PageDown, false, false) => Self::PageDown,
            (KeyCode::Backspace, false, false) => Self::Backspace,
            (KeyCode::Char('u'), true, false) => Self::ClearQuery,
            (KeyCode::Char('c'), false, true) => Self::ToggleCase,
            (KeyCode::Char('r'), false, true) => Self::ToggleRegex,
            (KeyCode::Char(c), false, false) => Self::Insert(c),
            _ => return None,
        })
    }
}

#[derive(Debug)]
pub(crate) struct Search {
    root: PathBuf,
    /// Which search this listens to; results of any other are ignored.
    id: u64,
    /// Stops the search when dropped.
    _handle: Option<SearchHandle>,
    query: String,
    options: Options,
    hits: Vec<Hit>,
    selected: usize,
    status: Status,
}

impl Search {
    /// A search of the project at `root`, with `query` typed already. Nothing
    /// is running until [`Search::begin`].
    pub(crate) fn new(root: PathBuf, query: String, options: Options) -> Self {
        Self {
            root,
            id: 0,
            _handle: None,
            query,
            options,
            hits: Vec::new(),
            selected: 0,
            status: Status::Idle,
        }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    pub(crate) fn options(&self) -> Options {
        self.options
    }

    #[cfg(test)]
    pub(crate) fn hits(&self) -> &[Hit] {
        &self.hits
    }

    #[cfg(test)]
    pub(crate) fn selected(&self) -> usize {
        self.selected
    }

    pub(crate) fn selected_hit(&self) -> Option<&Hit> {
        self.hits.get(self.selected)
    }

    #[cfg(test)]
    pub(crate) fn status(&self) -> &Status {
        &self.status
    }

    pub(crate) fn insert(&mut self, c: char) {
        self.query.push(c);
    }

    pub(crate) fn backspace(&mut self) {
        self.query.pop();
    }

    pub(crate) fn clear_query(&mut self) {
        self.query.clear();
    }

    pub(crate) fn toggle_case(&mut self) {
        self.options.case_sensitive = !self.options.case_sensitive;
    }

    pub(crate) fn toggle_regex(&mut self) {
        self.options.regex = !self.options.regex;
    }

    pub(crate) fn select_previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub(crate) fn select_next(&mut self) {
        self.selected = (self.selected + 1).min(self.hits.len().saturating_sub(1));
    }

    pub(crate) fn page_up(&mut self) {
        self.selected = self.selected.saturating_sub(PAGE);
    }

    pub(crate) fn page_down(&mut self) {
        self.selected = (self.selected + PAGE).min(self.hits.len().saturating_sub(1));
    }

    /// Forgets the results and listens to the search `id`.
    pub(crate) fn begin(&mut self, id: u64, handle: SearchHandle) {
        self.reset(Status::Running);
        self.id = id;
        self._handle = Some(handle);
    }

    /// Forgets the results and stops searching: there is nothing to look for.
    pub(crate) fn idle(&mut self) {
        self.reset(Status::Idle);
    }

    /// Forgets the results: the query cannot be searched for.
    pub(crate) fn invalid(&mut self, reason: String) {
        self.reset(Status::Invalid(reason));
    }

    fn reset(&mut self, status: Status) {
        self._handle = None;
        self.id = 0;
        self.hits.clear();
        self.selected = 0;
        self.status = status;
    }

    /// Takes results of the search this listens to; others are ignored.
    pub(crate) fn add_hits(&mut self, id: u64, hits: Vec<Hit>) {
        if id == self.id {
            self.hits.extend(hits);
        }
    }

    pub(crate) fn finish(&mut self, id: u64, files: usize, truncated: bool) {
        if id == self.id {
            self.status = Status::Done { files, truncated };
        }
    }

    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        let area = self.area(frame.area());
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border.focused"))
            .title(Line::styled(" Find in Project ", theme.style("pane.title")));
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);
        if inner.height < 4 {
            return;
        }
        let dim = theme.style("pane.border");

        let input_area = Rect { height: 1, ..inner };
        let prompt = format!("> {}", self.query);
        frame.render_widget(Paragraph::new(prompt.as_str()), input_area);
        let cursor_x = input_area.x + prompt.width().min(usize::from(input_area.width)) as u16;
        frame.set_cursor_position(Position::new(
            cursor_x.min(input_area.right()),
            input_area.y,
        ));

        let options_area = Rect {
            y: inner.y + 1,
            height: 1,
            ..inner
        };
        frame.render_widget(Paragraph::new(self.options_line()).style(dim), options_area);

        let footer_area = Rect {
            y: inner.bottom() - 1,
            height: 1,
            ..inner
        };
        frame.render_widget(Paragraph::new(self.footer()).style(dim), footer_area);

        let list_area = Rect {
            y: inner.y + 2,
            height: inner.height - 3,
            ..inner
        };
        if self.hits.is_empty() {
            let message = match self.status {
                Status::Done { .. } => " no results",
                _ => "",
            };
            frame.render_widget(Paragraph::new(message).style(dim), list_area);
            return;
        }
        let found = theme.style("search.match");
        let rows = self.hits.iter().map(|hit| row(hit, dim, found));
        let mut state = ListState::default().with_selected(Some(self.selected));
        let list = List::new(rows).highlight_style(theme.style("list.selected"));
        frame.render_stateful_widget(list, list_area, &mut state);
    }

    fn options_line(&self) -> String {
        let mark = |on: bool| if on { "x" } else { " " };
        format!(
            " [{}] match case (alt+c)   [{}] regex (alt+r)",
            mark(self.options.case_sensitive),
            mark(self.options.regex)
        )
    }

    fn footer(&self) -> String {
        match &self.status {
            Status::Idle => " type to search the project".to_owned(),
            Status::Running => format!(" searching... {} results", self.hits.len()),
            Status::Done { .. } if self.hits.is_empty() => " no results".to_owned(),
            Status::Done { files, truncated } => {
                let mut text = format!(" {} results in {files} files", self.hits.len());
                if *truncated {
                    text.push_str(&format!(" (stopped at {MAX_HITS}; narrow the search)"));
                }
                text
            }
            Status::Invalid(reason) => format!(" invalid pattern: {reason}"),
        }
    }

    /// The box: horizontally centered, near the top.
    fn area(&self, screen: Rect) -> Rect {
        let width = WIDTH.min(screen.width);
        let rows = (self.hits.len().max(1) as u16).min(MAX_ROWS);
        let height = (rows + 5).min(screen.height);
        Rect {
            x: screen.x + (screen.width - width) / 2,
            y: screen.y + (screen.height / 10).min(screen.height - height),
            width,
            height,
        }
    }
}

/// A row: where the match is, dimmed, then the line with the match marked.
fn row<'a>(hit: &Hit, dim: Style, found: Style) -> ListItem<'a> {
    let (before, matched, after) = (
        &hit.text[..hit.matched.start],
        &hit.text[hit.matched.clone()],
        &hit.text[hit.matched.end..],
    );
    ListItem::new(Line::from(vec![
        Span::styled(format!(" {}:{}", hit.path.display(), hit.line + 1), dim),
        Span::raw("  "),
        Span::raw(before.to_owned()),
        Span::styled(matched.to_owned(), found),
        Span::raw(after.to_owned()),
    ]))
}
