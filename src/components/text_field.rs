//! One line of text being typed, with a cursor that moves: what the prompts, the finder, the
//! palette, the find bar and the project search share. The text is in graphemes for the cursor
//! (a letter with its accent, a flag, is one step), in columns for what is drawn, and a text
//! longer than its box scrolls to keep the cursor in view.

use std::cell::Cell;

use crossterm::event::KeyCode;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::widgets::Paragraph;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::components::text::head_fitting;

/// Where a key moves the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Caret {
    Left,
    Right,
    Home,
    End,
    WordLeft,
    WordRight,
}

impl Caret {
    /// The move a key asks for, `ctrl` meaning by word: `left`, `right`, `home`, `end`.
    pub(crate) fn from_key(code: KeyCode, ctrl: bool) -> Option<Self> {
        Some(match (code, ctrl) {
            (KeyCode::Left, true) => Self::WordLeft,
            (KeyCode::Right, true) => Self::WordRight,
            (KeyCode::Left, false) => Self::Left,
            (KeyCode::Right, false) => Self::Right,
            (KeyCode::Home, _) => Self::Home,
            (KeyCode::End, _) => Self::End,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TextField {
    text: String,
    /// Where typing goes: a byte offset in `text`, always between two graphemes.
    cursor: usize,
    /// The first byte of `text` that was drawn last, kept while the cursor stays in view.
    scroll: Cell<usize>,
    /// The cells the text was drawn in last (after its prompt), for a click.
    drawn: Cell<Rect>,
}

impl TextField {
    /// A field holding `text`, with the cursor after it.
    pub(crate) fn with_text(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            cursor: text.len(),
            text,
            scroll: Cell::new(0),
            drawn: Cell::new(Rect::default()),
        }
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Where the cursor is: a byte offset in the text.
    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    /// Draws the field on the one row of `area`, after `prompt`, and puts the terminal's cursor
    /// at the field's.
    pub(crate) fn render_input(&self, frame: &mut Frame, area: Rect, prompt: &str) {
        let room = usize::from(area.width).saturating_sub(prompt.width() + 1);
        let (shown, before) = self.visible(room);
        let line = format!("{prompt}{shown}");
        frame.render_widget(Paragraph::new(line), area);
        let prompt_width = u16::try_from(prompt.width())
            .unwrap_or(u16::MAX)
            .min(area.width);
        self.set_drawn(Rect {
            x: area.x + prompt_width,
            width: area.width - prompt_width,
            height: area.height.min(1),
            ..area
        });
        let column = (prompt.width() + before).min(usize::from(u16::MAX)) as u16;
        frame.set_cursor_position(Position::new(
            area.x.saturating_add(column).min(area.right()),
            area.y,
        ));
    }

    /// Says where the text was drawn (the cells after any prompt), so that a click there can put
    /// the cursor in it.
    pub(crate) fn set_drawn(&self, text_area: Rect) {
        self.drawn.set(text_area);
    }

    /// A click at a screen cell: when it is on the cells the text was drawn in, puts the cursor
    /// at the nearer edge of the character there (past the end of the text: the end). Whether it
    /// was on them.
    pub(crate) fn click_at(&mut self, column: u16, row: u16) -> bool {
        let area = self.drawn.get();
        let inside = area.width > 0
            && area.height > 0
            && row == area.y
            && column >= area.x
            && column < area.right();
        if inside {
            self.click(usize::from(column - area.x));
        }
        inside
    }

    /// The first byte that was drawn last.
    #[cfg(test)]
    pub(crate) fn scroll(&self) -> usize {
        self.scroll.get()
    }

    /// Types `c` at the cursor.
    pub(crate) fn insert(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Deletes the character (grapheme) before the cursor. Whether there was one.
    pub(crate) fn backspace(&mut self) -> bool {
        let start = self.previous_boundary(self.cursor);
        let removed = start < self.cursor;
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
        removed
    }

    /// Deletes the character (grapheme) at the cursor. Whether there was one.
    pub(crate) fn delete(&mut self) -> bool {
        let end = self.next_boundary(self.cursor);
        let removed = end > self.cursor;
        self.text.replace_range(self.cursor..end, "");
        removed
    }

    pub(crate) fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    /// Replaces the whole text, with the cursor after it.
    pub(crate) fn set(&mut self, text: &str) {
        self.text.clear();
        self.text.push_str(text);
        self.cursor = self.text.len();
    }

    /// Replaces what is after the first `keep` bytes with `text`, the cursor after it.
    pub(crate) fn replace_from(&mut self, keep: usize, text: &str) {
        self.text.truncate(keep);
        self.text.push_str(text);
        self.cursor = self.text.len();
    }

    /// Cuts out the bytes `start..end`, the cursor going to where they were.
    pub(crate) fn remove_range(&mut self, start: usize, end: usize) {
        self.text.replace_range(start..end, "");
        self.cursor = start;
    }

    /// The text before the cursor.
    pub(crate) fn before_cursor(&self) -> &str {
        &self.text[..self.cursor]
    }

    /// Moves the cursor as `caret` says.
    pub(crate) fn move_caret(&mut self, caret: Caret) {
        match caret {
            Caret::Left => self.move_left(),
            Caret::Right => self.move_right(),
            Caret::Home => self.cursor = 0,
            Caret::End => self.cursor = self.text.len(),
            Caret::WordLeft => self.move_word_left(),
            Caret::WordRight => self.move_word_right(),
        }
    }

    pub(crate) fn move_left(&mut self) {
        self.cursor = self.previous_boundary(self.cursor);
    }

    pub(crate) fn move_right(&mut self) {
        self.cursor = self.next_boundary(self.cursor);
    }

    /// To the start of the word before the cursor: over what is not a word, then over the
    /// word.
    pub(crate) fn move_word_left(&mut self) {
        let mut at = self.cursor;
        while at > 0 && !self.is_word_before(at) {
            at = self.previous_boundary(at);
        }
        while at > 0 && self.is_word_before(at) {
            at = self.previous_boundary(at);
        }
        self.cursor = at;
    }

    /// To the end of the word after the cursor: over what is not a word, then over the word.
    pub(crate) fn move_word_right(&mut self) {
        let mut at = self.cursor;
        while at < self.text.len() && !self.is_word_at(at) {
            at = self.next_boundary(at);
        }
        while at < self.text.len() && self.is_word_at(at) {
            at = self.next_boundary(at);
        }
        self.cursor = at;
    }

    /// The text that is drawn in `room` columns, and how many columns of it are before the
    /// cursor. A text that fits is drawn whole; a longer one starts as far along as the cursor
    /// needs, and stays where it was while the cursor is in view.
    pub(crate) fn visible(&self, room: usize) -> (&str, usize) {
        let from = self.first_shown(room);
        self.scroll.set(from);
        let shown = head_fitting(&self.text[from..], room);
        (shown, self.text[from..self.cursor].width())
    }

    /// Puts the cursor at the character that is `offset` columns into what was drawn last (the
    /// nearer edge of a wide one). An offset past the text puts it at the end.
    pub(crate) fn click(&mut self, offset: usize) {
        let mut used = 0;
        let mut at = self.floor_to_boundary(self.scroll.get());
        for grapheme in self.text[at..].graphemes(true) {
            let width = grapheme.width().max(1);
            if offset < used + width {
                if offset - used >= width.div_ceil(2) {
                    at += grapheme.len();
                }
                break;
            }
            used += width;
            at += grapheme.len();
        }
        self.cursor = at;
    }

    fn first_shown(&self, room: usize) -> usize {
        let mut from = self.floor_to_boundary(self.scroll.get().min(self.cursor));
        while from < self.cursor && self.text[from..self.cursor].width() > room {
            from = self.next_boundary(from);
        }
        // Nothing needs hiding when the whole text fits.
        if self.text.width() <= room { 0 } else { from }
    }

    fn previous_boundary(&self, from: usize) -> usize {
        self.text[..from]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(index, _)| index)
    }

    fn next_boundary(&self, from: usize) -> usize {
        self.text[from..]
            .graphemes(true)
            .next()
            .map_or(from, |grapheme| from + grapheme.len())
    }

    fn is_word_before(&self, at: usize) -> bool {
        self.text[..at].chars().next_back().is_some_and(is_word)
    }

    fn is_word_at(&self, at: usize) -> bool {
        self.text[at..].chars().next().is_some_and(is_word)
    }

    /// The nearest grapheme boundary at or before `at`. A byte offset kept from an earlier
    /// frame may fall inside a character once the text was edited.
    fn floor_to_boundary(&self, at: usize) -> usize {
        if at >= self.text.len() {
            return self.text.len();
        }
        self.text
            .grapheme_indices(true)
            .map(|(index, _)| index)
            .take_while(|index| *index <= at)
            .last()
            .unwrap_or(0)
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests;
