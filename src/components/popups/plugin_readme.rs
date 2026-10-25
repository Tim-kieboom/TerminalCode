//! What a plugin says about itself: its description, where it comes from and its
//! README, to read and scroll. Opened with `d` in the plugin manager; `esc` goes back.

use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::components::popups::plain_key;
use crate::components::text::wrap_indented;
use crate::ui::backdrop;
use crate::ui::theme::Theme;

const HELP: &str = " up/down, pgup/pgdn: scroll | esc: back ";

/// The README, as far as it is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Body {
    Loading,
    Text(String),
    /// There is none, or it could not be fetched: why.
    Missing(String),
}

/// What a key asks the page to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Scroll(isize),
    Page(isize),
    Top,
    Bottom,
    Close,
}

impl Command {
    pub(crate) fn from_key(key: KeyEvent) -> Option<Self> {
        let code = plain_key(key)?;
        Some(match code {
            KeyCode::Up | KeyCode::Char('k') => Self::Scroll(-1),
            KeyCode::Down | KeyCode::Char('j') => Self::Scroll(1),
            KeyCode::PageUp => Self::Page(-1),
            KeyCode::PageDown | KeyCode::Char(' ') => Self::Page(1),
            KeyCode::Home | KeyCode::Char('g') => Self::Top,
            KeyCode::End | KeyCode::Char('G') => Self::Bottom,
            KeyCode::Esc | KeyCode::Char('q' | 'd') | KeyCode::Enter => Self::Close,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Readme {
    /// The plugin this is about, to go back to its row.
    id: Box<str>,
    /// Where the README is, so that it can be filled in when it arrives.
    url: Option<String>,
    title: String,
    /// The lines above the README: what the manager knows.
    head: Vec<String>,
    body: Body,
    scroll: usize,
    /// How far it can scroll, as of the last drawing.
    max_scroll: Cell<usize>,
    /// How many lines a page is, as of the last drawing.
    page: Cell<usize>,
}

impl Readme {
    pub(crate) fn new(
        id: &str,
        url: Option<String>,
        title: String,
        head: Vec<String>,
        body: Body,
    ) -> Self {
        Self {
            id: id.into(),
            url,
            title,
            head,
            body,
            scroll: 0,
            max_scroll: Cell::new(0),
            page: Cell::new(10),
        }
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub(crate) fn set_body(&mut self, body: Body) {
        self.body = body;
    }

    pub(crate) fn apply(&mut self, command: Command) {
        let page = self.page.get().max(1) as isize;
        let by = match command {
            Command::Scroll(by) => by,
            Command::Page(pages) => pages * page,
            Command::Top => isize::MIN / 2,
            Command::Bottom => isize::MAX / 2,
            Command::Close => return,
        };
        let wanted = (self.scroll as isize).saturating_add(by).max(0) as usize;
        self.scroll = wanted.min(self.max_scroll.get());
    }

    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        let screen = frame.area();
        let width = screen
            .width
            .saturating_sub(8)
            .clamp(30, 110)
            .min(screen.width);
        let height = screen.height.saturating_sub(4).max(8).min(screen.height);
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
            .title(Line::styled(
                format!(" {} ", self.title),
                theme.style("pane.title"),
            ))
            .title_bottom(Line::styled(HELP, theme.style("tab.inactive")));
        let inner = block.inner(area);
        backdrop::clear(frame, theme, area);
        frame.render_widget(block, area);

        let text_width = usize::from(inner.width).saturating_sub(2).max(1);
        let lines = self.lines(text_width, theme);
        let rows = usize::from(inner.height);
        let max = lines.len().saturating_sub(rows);
        self.max_scroll.set(max);
        self.page.set(rows.saturating_sub(1));
        let start = self.scroll.min(max);
        let shown: Vec<Line> = lines.into_iter().skip(start).take(rows).collect();
        frame.render_widget(Paragraph::new(shown), inner);
        if let Some(place) = (start * rows.saturating_sub(1)).checked_div(max) {
            frame.render_widget(
                Paragraph::new(Line::styled("┃", theme.style("tab.inactive"))),
                Rect {
                    x: inner.x + inner.width - 1,
                    y: inner.y + place as u16,
                    width: 1,
                    height: 1,
                },
            );
        }
    }

    /// Every line of the page, wrapped to `width`.
    fn lines(&self, width: usize, theme: &Theme) -> Vec<Line<'static>> {
        let text = theme.style("explorer.file");
        let hint = theme.style("explorer.hint");
        let heading = theme.style("pane.title").add_modifier(Modifier::BOLD);
        let mut lines: Vec<Line<'static>> = Vec::new();
        let push = |lines: &mut Vec<Line<'static>>, text_of: &str, style| {
            for piece in wrap_indented(text_of, width) {
                lines.push(Line::from(Span::styled(format!(" {piece}"), style)));
            }
        };
        for line in &self.head {
            push(&mut lines, line, hint);
        }
        if !self.head.is_empty() {
            lines.push(Line::default());
        }
        match &self.body {
            Body::Loading => push(&mut lines, "Fetching the description...", hint),
            Body::Missing(why) => push(&mut lines, why, hint),
            Body::Text(markdown) => {
                let mut fenced = false;
                for line in markdown.lines() {
                    if line.trim_start().starts_with("```") {
                        fenced = !fenced;
                        continue;
                    }
                    match (fenced, line.trim_start_matches('#')) {
                        (true, _) => push(&mut lines, &format!("  {line}"), hint),
                        (false, rest) if rest.len() != line.len() && line.starts_with('#') => {
                            push(&mut lines, rest.trim(), heading)
                        }
                        (false, _) => push(&mut lines, &plain(line), text),
                    }
                }
            }
        }
        lines
    }
}

/// A line of Markdown without the marks that only make it look like Markdown.
fn plain(line: &str) -> String {
    line.replace("**", "").replace('`', "")
}

#[cfg(test)]
mod tests;
