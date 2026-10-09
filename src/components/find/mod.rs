//! Find in the open file: every match is highlighted, the current one is
//! selected, and the keys step through them.
//!
//! The query is literal text unless regex is on and ignores case unless match
//! case is on, the same as the project search.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Position as ScreenPosition, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use regex::{Regex, RegexBuilder};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::buffer::{Buffer, Position};
use crate::components::search::Options;
use crate::ui::theme::Theme;

#[cfg(test)]
mod tests;

/// Matches kept; a query like `e` in a big file would otherwise make every
/// keystroke slow and the highlights meaningless.
pub(crate) const MAX_MATCHES: usize = 10_000;
/// The longest selection that becomes the query when the bar opens.
const MAX_SEEDED_QUERY: usize = 200;

/// A match on one line, as grapheme columns (`end` is exclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Match {
    pub(crate) line: usize,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

impl Match {
    pub(crate) fn start_position(&self) -> Position {
        Position::new(self.line, self.start)
    }

    pub(crate) fn end_position(&self) -> Position {
        Position::new(self.line, self.end)
    }
}

/// What a key press asks the find bar to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Insert(char),
    Backspace,
    ClearQuery,
    ToggleCase,
    ToggleRegex,
    Next,
    Previous,
    Close,
}

impl Command {
    pub(crate) fn from_key(key: KeyEvent) -> Option<Self> {
        if key.kind == KeyEventKind::Release {
            return None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        Some(match (key.code, ctrl, alt, shift) {
            (KeyCode::Esc, false, false, _) => Self::Close,
            (KeyCode::Enter, false, false, false) => Self::Next,
            (KeyCode::Enter, false, false, true) => Self::Previous,
            (KeyCode::Down, false, false, _) => Self::Next,
            (KeyCode::Up, false, false, _) => Self::Previous,
            (KeyCode::F(3), false, false, false) => Self::Next,
            (KeyCode::F(3), false, false, true) => Self::Previous,
            (KeyCode::Backspace, false, false, _) => Self::Backspace,
            (KeyCode::Char('u'), true, false, _) => Self::ClearQuery,
            (KeyCode::Char('c'), false, true, _) => Self::ToggleCase,
            (KeyCode::Char('r'), false, true, _) => Self::ToggleRegex,
            (KeyCode::Char(c), false, false, _) => Self::Insert(c),
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Problem {
    /// The query is not a valid pattern; the text says why.
    InvalidPattern(String),
}

#[derive(Debug, Clone)]
pub(crate) struct Find {
    query: String,
    options: Options,
    /// In the order they appear in the file.
    matches: Vec<Match>,
    capped: bool,
    current: Option<usize>,
    problem: Option<Problem>,
    /// Where the next match is looked for from when the query changes: where
    /// the cursor was, then the start of the match last stepped to.
    origin: Position,
    /// The buffer version the matches belong to.
    version: u64,
}

impl Find {
    /// Starts finding in `buffer` for `query`, beginning at `origin`.
    pub(crate) fn open(buffer: &Buffer, origin: Position, query: String, options: Options) -> Self {
        let mut find = Self {
            query,
            options,
            matches: Vec::new(),
            capped: false,
            current: None,
            problem: None,
            origin,
            version: buffer.version(),
        };
        find.recompute(buffer);
        find
    }

    /// The text to start a find with: the selection, if it is one short line
    /// of text.
    pub(crate) fn seed(buffer: &Buffer, start: Position, end: Position) -> Option<String> {
        if start.line != end.line || start == end {
            return None;
        }
        let line = buffer.line_content(start.line).ok()?;
        let text: String = line
            .graphemes(true)
            .skip(start.column)
            .take(end.column - start.column)
            .collect();
        let usable = !text.trim().is_empty() && text.chars().count() <= MAX_SEEDED_QUERY;
        usable.then_some(text)
    }

    pub(crate) fn query(&self) -> &str {
        &self.query
    }

    pub(crate) fn options(&self) -> Options {
        self.options
    }

    #[cfg(test)]
    pub(crate) fn matches(&self) -> &[Match] {
        &self.matches
    }

    pub(crate) fn current(&self) -> Option<Match> {
        self.matches.get(self.current?).copied()
    }

    #[cfg(test)]
    pub(crate) fn current_index(&self) -> Option<usize> {
        self.current
    }

    /// The matches on `line`, for drawing.
    pub(crate) fn matches_on_line(&self, line: usize) -> &[Match] {
        let from = self.matches.partition_point(|found| found.line < line);
        let to = self.matches.partition_point(|found| found.line <= line);
        &self.matches[from..to]
    }

    #[cfg(test)]
    pub(crate) fn problem(&self) -> Option<&str> {
        match self.problem.as_ref()? {
            Problem::InvalidPattern(reason) => Some(reason),
        }
    }

    pub(crate) fn insert(&mut self, buffer: &Buffer, c: char) {
        self.query.push(c);
        self.recompute(buffer);
    }

    pub(crate) fn backspace(&mut self, buffer: &Buffer) {
        self.query.pop();
        self.recompute(buffer);
    }

    pub(crate) fn clear_query(&mut self, buffer: &Buffer) {
        self.query.clear();
        self.recompute(buffer);
    }

    pub(crate) fn toggle_case(&mut self, buffer: &Buffer) {
        self.options.case_sensitive = !self.options.case_sensitive;
        self.recompute(buffer);
    }

    pub(crate) fn toggle_regex(&mut self, buffer: &Buffer) {
        self.options.regex = !self.options.regex;
        self.recompute(buffer);
    }

    /// Looks for the matches again if the text changed since they were found.
    pub(crate) fn refresh(&mut self, buffer: &Buffer) {
        if self.version != buffer.version() {
            self.recompute(buffer);
        }
    }

    /// Steps to the next match, wrapping around from the last to the first.
    pub(crate) fn next(&mut self) {
        let Some(current) = self.current else {
            return;
        };
        self.current = Some((current + 1) % self.matches.len());
        self.settle();
    }

    /// Steps to the previous match, wrapping around.
    pub(crate) fn previous(&mut self) {
        let Some(current) = self.current else {
            return;
        };
        self.current = Some((current + self.matches.len() - 1) % self.matches.len());
        self.settle();
    }

    /// Makes the current match where later queries start looking from.
    fn settle(&mut self) {
        if let Some(found) = self.current() {
            self.origin = found.start_position();
        }
    }

    /// Finds the matches again and picks the first at or after the origin
    /// (the first of all when there is none after it).
    fn recompute(&mut self, buffer: &Buffer) {
        self.version = buffer.version();
        self.matches.clear();
        self.capped = false;
        self.problem = None;
        self.current = None;
        if self.query.is_empty() {
            return;
        }
        let regex = match build(&self.query, self.options) {
            Ok(regex) => regex,
            Err(reason) => {
                self.problem = Some(Problem::InvalidPattern(reason));
                return;
            }
        };
        self.capped = collect(buffer, &regex, &mut self.matches);

        let origin = self.origin;
        let at_or_after = self
            .matches
            .partition_point(|found| found.start_position() < origin);
        self.current = match self.matches.len() {
            0 => None,
            len => Some(at_or_after % len),
        };
    }

    /// Draws the bar into `area` (one row), with the terminal cursor after
    /// the query.
    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme, area: Rect) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let bar = theme.style("find.bar");
        let dim = theme.style("find.hint");
        let prompt = format!(" Find: {}", self.query);
        let status = self.status_text();
        let mark = |on: bool| if on { "x" } else { " " };
        let options = format!(
            "  [{}] case (alt+c)  [{}] regex (alt+r) ",
            mark(self.options.case_sensitive),
            mark(self.options.regex)
        );

        let used = prompt.width() + status.width() + options.width();
        let gap = usize::from(area.width).saturating_sub(used).max(1);
        let status_style = match self.problem {
            Some(_) => theme.style("notification.error"),
            None => bar,
        };
        let line = Line::from(vec![
            Span::styled(prompt.clone(), bar),
            Span::styled(" ".repeat(gap), bar),
            Span::styled(status, status_style),
            Span::styled(options, dim),
        ]);
        frame.render_widget(Paragraph::new(line).style(bar), area);

        let cursor_x = area.x + prompt.width().min(usize::from(area.width) - 1) as u16;
        frame.set_cursor_position(ScreenPosition::new(cursor_x, area.y));
    }

    fn status_text(&self) -> String {
        if let Some(Problem::InvalidPattern(reason)) = &self.problem {
            return format!("invalid pattern: {reason}");
        }
        if self.query.is_empty() {
            return String::new();
        }
        match self.current {
            Some(index) if self.capped => format!("{} of {}+", index + 1, self.matches.len()),
            Some(index) => format!("{} of {}", index + 1, self.matches.len()),
            None => "no results".to_owned(),
        }
    }
}

/// The pattern for `query`: the text itself unless regex is on.
fn build(query: &str, options: Options) -> Result<Regex, String> {
    let pattern = match options.regex {
        true => query.to_owned(),
        false => regex::escape(query),
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!options.case_sensitive)
        .build()
        .map_err(|error| short_message(&error.to_string()))
}

/// The part of a pattern error worth showing on one line.
fn short_message(message: &str) -> String {
    message
        .lines()
        .find_map(|line| line.trim().strip_prefix("error: "))
        .or_else(|| message.lines().next())
        .unwrap_or(message)
        .to_owned()
}

/// Adds the matches of `regex` in `buffer`, line by line, skipping empty
/// ones. Returns whether it stopped at [`MAX_MATCHES`].
fn collect(buffer: &Buffer, regex: &Regex, out: &mut Vec<Match>) -> bool {
    for line in 0..buffer.len_lines() {
        let Ok(content) = buffer.line_content(line) else {
            continue;
        };
        for found in regex.find_iter(&content) {
            if found.is_empty() {
                continue;
            }
            let start = content[..found.start()].graphemes(true).count();
            let end = start + content[found.start()..found.end()].graphemes(true).count();
            out.push(Match { line, start, end });
            if out.len() >= MAX_MATCHES {
                return true;
            }
        }
    }
    false
}
