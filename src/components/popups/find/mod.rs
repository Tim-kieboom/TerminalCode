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
use crate::components::editor::Replacement;
use crate::components::popups::search::Options;
use crate::components::text_field::{Caret, TextField};
use crate::ui::theme::Theme;

#[cfg(test)]
mod tests;

/// Matches kept; a query like `e` in a big file would otherwise make every
/// keystroke slow and the highlights meaningless.
pub(crate) const MAX_MATCHES: usize = 10_000;
/// The longest selection that becomes the query when the bar opens.
const MAX_SEEDED_QUERY: usize = 200;
/// The columns the shortest form of the three switches takes, and the least a text gets.
const SHORTEST_OPTIONS: usize = 14;
const MOST_NARROW_FIELD: usize = 8;

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
    Delete,
    /// Moves the text cursor in the text being typed.
    Caret(Caret),
    ClearQuery,
    ToggleCase,
    ToggleRegex,
    ToggleWord,
    /// Enter: the next match in the query field, replacing the current one in the
    /// replacement field.
    Confirm,
    Next,
    Previous,
    /// Shows or hides the row for the replacement.
    ToggleReplace,
    /// Moves between the query and the replacement.
    SwitchField,
    ReplaceAll,
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
        if !alt && let Some(caret) = Caret::from_key(key.code, ctrl) {
            return Some(Self::Caret(caret));
        }
        Some(match (key.code, ctrl, alt, shift) {
            (KeyCode::Esc, false, false, _) => Self::Close,
            (KeyCode::Delete, false, false, _) => Self::Delete,
            (KeyCode::Enter, false, false, false) => Self::Confirm,
            (KeyCode::Enter, false, false, true) => Self::Previous,
            (KeyCode::Down, false, false, _) => Self::Next,
            (KeyCode::Up, false, false, _) => Self::Previous,
            (KeyCode::F(3), false, false, false) => Self::Next,
            (KeyCode::F(3), false, false, true) => Self::Previous,
            (KeyCode::Backspace, false, false, _) => Self::Backspace,
            (KeyCode::Char('u'), true, false, _) => Self::ClearQuery,
            (KeyCode::Char('c'), false, true, _) => Self::ToggleCase,
            (KeyCode::Char('r'), false, true, _) => Self::ToggleRegex,
            (KeyCode::Char('w'), false, true, _) => Self::ToggleWord,
            (KeyCode::Char('a'), false, true, _) => Self::ReplaceAll,
            (KeyCode::Char('r'), true, false, _) => Self::ToggleReplace,
            (KeyCode::Tab | KeyCode::BackTab, false, false, _) => Self::SwitchField,
            (KeyCode::Char(c), false, false, _) => Self::Insert(c),
            _ => return None,
        })
    }
}

/// Which text the typing goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Query,
    Replacement,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Problem {
    /// The query is not a valid pattern; the text says why.
    InvalidPattern(String),
}

#[derive(Debug, Clone)]
pub(crate) struct Find {
    query: TextField,
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
    /// What a match is replaced with; with regex on, `$1` and `${name}` stand
    /// for the groups of the match.
    replacement: TextField,
    /// Whether the row for the replacement is shown.
    replace_open: bool,
    /// Which of the two the typing goes to.
    active: Field,
}

impl Find {
    /// Starts finding in `buffer` for `query`, beginning at `origin`.
    pub(crate) fn open(buffer: &Buffer, origin: Position, query: String, options: Options) -> Self {
        let mut find = Self {
            query: TextField::with_text(query),
            options,
            matches: Vec::new(),
            capped: false,
            current: None,
            problem: None,
            origin,
            version: buffer.version(),
            replacement: TextField::default(),
            replace_open: false,
            active: Field::Query,
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
        self.query.text()
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

    /// The text the typing goes to.
    fn typing(&mut self) -> &mut TextField {
        match self.active {
            Field::Query => &mut self.query,
            Field::Replacement => &mut self.replacement,
        }
    }

    /// The matches are looked for again when the query is what changed.
    fn edited(&mut self, buffer: &Buffer) {
        if self.active == Field::Query {
            self.recompute(buffer);
        }
    }

    pub(crate) fn insert(&mut self, buffer: &Buffer, c: char) {
        self.typing().insert(c);
        self.edited(buffer);
    }

    pub(crate) fn backspace(&mut self, buffer: &Buffer) {
        if self.typing().backspace() {
            self.edited(buffer);
        }
    }

    /// Deletes the character at the cursor of the text being typed.
    pub(crate) fn delete(&mut self, buffer: &Buffer) {
        if self.typing().delete() {
            self.edited(buffer);
        }
    }

    /// Moves the cursor in the text being typed.
    pub(crate) fn move_caret(&mut self, caret: Caret) {
        self.typing().move_caret(caret);
    }

    /// Empties the text being typed.
    pub(crate) fn clear_query(&mut self, buffer: &Buffer) {
        self.typing().clear();
        self.edited(buffer);
    }

    pub(crate) fn toggle_case(&mut self, buffer: &Buffer) {
        self.options.case_sensitive = !self.options.case_sensitive;
        self.recompute(buffer);
    }

    pub(crate) fn toggle_regex(&mut self, buffer: &Buffer) {
        self.options.regex = !self.options.regex;
        self.recompute(buffer);
    }

    pub(crate) fn toggle_word(&mut self, buffer: &Buffer) {
        self.options.whole_word = !self.options.whole_word;
        self.recompute(buffer);
    }

    pub(crate) fn replacement(&self) -> &str {
        self.replacement.text()
    }

    pub(crate) fn set_replacement(&mut self, text: String) {
        self.replacement.set(&text);
    }

    #[cfg(test)]
    pub(crate) fn replace_open(&self) -> bool {
        self.replace_open
    }

    /// Whether typing goes to the replacement, which Enter then puts in.
    pub(crate) fn replacing(&self) -> bool {
        self.replace_open && self.active == Field::Replacement
    }

    /// The rows the bar takes.
    pub(crate) fn bar_rows(&self) -> u16 {
        match self.replace_open {
            true => 2,
            false => 1,
        }
    }

    /// Shows the replacement row and gives it the typing, or hides it again.
    pub(crate) fn toggle_replace(&mut self) {
        self.replace_open = !self.replace_open;
        self.active = match self.replace_open {
            true => Field::Replacement,
            false => Field::Query,
        };
    }

    /// Moves the typing between the query and the replacement, if both are shown.
    pub(crate) fn switch_field(&mut self) {
        if !self.replace_open {
            return;
        }
        self.active = match self.active {
            Field::Query => Field::Replacement,
            Field::Replacement => Field::Query,
        };
    }

    /// What replacing the current match would put in its place.
    pub(crate) fn current_replacement(&self, buffer: &Buffer) -> Option<Replacement> {
        let found = self.current()?;
        let content = buffer.line_content(found.line).ok()?;
        let regex = build(self.query.text(), self.options).ok()?;
        let on_line = self.line_replacements(&regex, &content);
        let (start, end, text) = on_line
            .into_iter()
            .find(|(start, end, _)| (*start, *end) == (found.start, found.end))?;
        Some(Replacement {
            start: Position::new(found.line, start),
            end: Position::new(found.line, end),
            text,
        })
    }

    /// What replacing every match would put in their places, in text order.
    pub(crate) fn all_replacements(&self, buffer: &Buffer) -> Vec<Replacement> {
        let Ok(regex) = build(self.query.text(), self.options) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(self.matches.len());
        let mut line = None;
        let mut on_line = Vec::new();
        for found in &self.matches {
            if line != Some(found.line) {
                line = Some(found.line);
                let Ok(content) = buffer.line_content(found.line) else {
                    on_line.clear();
                    continue;
                };
                on_line = self.line_replacements(&regex, &content);
            }
            if let Some((start, end, text)) = on_line
                .iter()
                .find(|(start, end, _)| (*start, *end) == (found.start, found.end))
            {
                out.push(Replacement {
                    start: Position::new(found.line, *start),
                    end: Position::new(found.line, *end),
                    text: text.clone(),
                });
            }
        }
        out
    }

    /// How many matches there are, and whether more were left out.
    pub(crate) fn match_count(&self) -> (usize, bool) {
        (self.matches.len(), self.capped)
    }

    /// The matches of `regex` on one line, as grapheme columns with the text
    /// that replaces each. Without regex the replacement is taken as it is.
    fn line_replacements(&self, regex: &Regex, content: &str) -> Vec<(usize, usize, String)> {
        regex
            .captures_iter(content)
            .filter_map(|caps| {
                let whole = caps.get(0).filter(|whole| !whole.is_empty())?;
                let start = content[..whole.start()].graphemes(true).count();
                let end = start + whole.as_str().graphemes(true).count();
                let text = match self.options.regex {
                    true => {
                        let mut text = String::new();
                        caps.expand(self.replacement.text(), &mut text);
                        text
                    }
                    false => self.replacement.text().to_owned(),
                };
                Some((start, end, text))
            })
            .collect()
    }

    /// After a replacement the next match is looked for from `origin`, which is
    /// where the replaced text ends.
    pub(crate) fn continue_from(&mut self, buffer: &Buffer, origin: Position) {
        self.origin = origin;
        self.recompute(buffer);
    }

    /// Goes on in another file (or from another place): the matches are looked for again from
    /// `origin`.
    pub(crate) fn retarget(&mut self, buffer: &Buffer, origin: Position) {
        self.origin = origin;
        self.recompute(buffer);
    }

    /// Steps to the match after (or before) where the cursor is, `start` and `end` being the
    /// selection: the ordinary step when it is on the current match, else the first match that
    /// starts at or after the end of the selection (the last that starts before it), wrapping.
    pub(crate) fn step_from(&mut self, start: Position, end: Position, forward: bool) {
        if self.matches.is_empty() {
            return;
        }
        if let Some(current) = self.current()
            && current.start_position() == start
            && current.end_position() == end
        {
            match forward {
                true => self.next(),
                false => self.previous(),
            }
            return;
        }
        let len = self.matches.len();
        let index = match forward {
            true => {
                self.matches
                    .partition_point(|found| found.start_position() < end)
                    % len
            }
            false => {
                let before = self
                    .matches
                    .partition_point(|found| found.start_position() < start);
                (before + len - 1) % len
            }
        };
        self.current = Some(index);
        self.settle();
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
        let regex = match build(self.query.text(), self.options) {
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

    /// Draws the bar into `area` (one row, or two with the replacement), with
    /// the terminal cursor after the text being typed.
    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme, area: Rect, focused: bool) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let bar = theme.style("find.bar");
        let dim = theme.style("find.hint");
        let first = Rect { height: 1, ..area };
        let status = self.status_text();
        // The query takes what the status and the shortest form of the switches leave.
        let label = " Find: ";
        let query_room = usize::from(area.width)
            .saturating_sub(label.width() + status.width() + SHORTEST_OPTIONS)
            .max(MOST_NARROW_FIELD);
        let (shown, before) = self.query.visible(query_room);
        let prompt = format!("{label}{shown}");
        let query_cursor = label.width() + before;
        let room = usize::from(area.width).saturating_sub(prompt.width() + status.width() + 1);
        let options = self.options_text(room);

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
        frame.render_widget(Paragraph::new(line).style(bar), first);

        let mut cursor = (first.y, query_cursor);
        if self.replace_open && area.height > 1 {
            let second = Rect {
                y: area.y + 1,
                height: 1,
                ..area
            };
            let hint = "enter replaces, alt+a replaces all, tab switches ";
            let label = " Replace: ";
            let room = usize::from(area.width)
                .saturating_sub(label.width() + hint.width())
                .max(MOST_NARROW_FIELD);
            let (shown, before) = self.replacement.visible(room);
            let prompt = format!("{label}{shown}");
            let gap = usize::from(area.width)
                .saturating_sub(prompt.width() + hint.width())
                .max(1);
            let line = Line::from(vec![
                Span::styled(prompt.clone(), bar),
                Span::styled(" ".repeat(gap), bar),
                Span::styled(hint, dim),
            ]);
            frame.render_widget(Paragraph::new(line).style(bar), second);
            if self.active == Field::Replacement {
                cursor = (second.y, label.width() + before);
            }
        }

        // The terminal's cursor is the editor's while the keys are there.
        if focused {
            let cursor_x = area.x + cursor.1.min(usize::from(area.width) - 1) as u16;
            frame.set_cursor_position(ScreenPosition::new(cursor_x, cursor.0));
        }
    }

    /// The three switches, in the longest form that fits in `room` cells: with
    /// their keys, then without them, then just the letters.
    fn options_text(&self, room: usize) -> String {
        let mark = |on: bool| if on { "x" } else { " " };
        let [case, word, regex] = [
            mark(self.options.case_sensitive),
            mark(self.options.whole_word),
            mark(self.options.regex),
        ];
        let forms = [
            format!("  [{case}] case (alt+c)  [{word}] word (alt+w)  [{regex}] regex (alt+r) "),
            format!("  [{case}] case  [{word}] word  [{regex}] regex "),
            format!(" [{case}]c [{word}]w [{regex}]r "),
        ];
        let [full, plain, short] = forms;
        [full, plain]
            .into_iter()
            .find(|form| form.width() <= room)
            .unwrap_or(short)
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
    // Whole words: nothing word-like touches the match on either side, which
    // is also true for a match that starts or ends in punctuation.
    let pattern = match options.whole_word {
        true => format!(r"\b{{start-half}}(?:{pattern})\b{{end-half}}"),
        false => pattern,
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
