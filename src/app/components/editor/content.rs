use std::{fs, path::Path, vec};

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    StartupArgs,
    app::components::{
        Component,
        utils::cursor_scroller::{CursorScroller, Position, ScrollMode},
    },
    keybinds::{Action, PanelContext},
    theme::Theme,
};

#[cfg(test)]
#[path = "content_tests.rs"]
mod tests;

pub struct Content {
    pub(super) context: String,
    pub(super) scroller: CursorScroller,
    preferred_horizontal: usize,
}

impl Content {
    pub fn new(_args: &StartupArgs) -> Self {
        Self {
            context: "".to_string(),
            scroller: CursorScroller::new(ScrollMode::TextEditor),
            preferred_horizontal: 0,
        }
    }

    pub fn open(&mut self, path: &Path) -> std::io::Result<()> {
        let text = fs::read_to_string(path)?;
        self.context = text.replace("\r\n", "\n");
        self.set_cursor(0, 0);
        Ok(())
    }

    pub fn insert_char(&mut self, ch: char) {
        let mut lines = self.lines_vec();
        let Position {
            vertical,
            horizontal,
        } = self.get_position(lines.len());

        let line = &mut lines[vertical];
        let mut chars: Vec<char> = line.chars().collect();

        let posistion = horizontal.min(chars.len());
        chars.insert(posistion, ch);
        *line = chars.into_iter().collect();

        self.context = lines.join("\n");
        self.set_cursor(vertical, posistion + 1);
    }

    pub fn delete_char(&mut self) {
        let mut lines = self.lines_vec();
        let Position {
            vertical,
            horizontal,
        } = self.get_position(lines.len());

        if horizontal < lines[vertical].chars().count() {
            let mut chars: Vec<char> = lines[vertical].chars().collect();
            chars.remove(horizontal);
            lines[vertical] = chars.into_iter().collect();
        } else if vertical + 1 < lines.len() {
            let next = lines.remove(vertical + 1);
            lines[vertical] = format!("{}{}", lines[vertical], next);
        }

        self.context = lines.join("\n");
        let clamped = horizontal.min(lines[vertical].chars().count());
        self.set_cursor(vertical, clamped);
    }

    pub fn insert_newline(&mut self) {
        let mut lines = self.lines_vec();
        let Position {
            vertical,
            horizontal,
        } = self.get_position(lines.len());

        let (before, after) = chars_split(&lines[vertical], horizontal);
        lines[vertical] = before;
        lines.insert(vertical + 1, after);

        self.context = lines.join("\n");
        self.set_cursor(vertical + 1, 0);
    }

    pub fn insert_tab(&mut self) {
        for _ in 0..4 {
            self.insert_char(' ');
        }
    }

    pub fn backspace(&mut self) {
        let mut lines = self.lines_vec();
        let position = self.get_position(lines.len());

        if position.horizontal > 0 {
            self.remove_char(&mut lines, position);
        } else if position.vertical > 0 {
            self.remove_line(&mut lines, position);
        }

        self.context = lines.join("\n");
    }

    pub fn move_curser(&mut self, action: Action) {
        let lines_len = self.lines().count();
        let vertical = self.scroller.vertical();
        let line_len = self.line_len(vertical);

        match action {
            Action::ScrollWordLeft => {
                self.scroll_word_left(lines_len, line_len);
            }
            Action::ScrollWordRight => {
                self.scroll_word_right(lines_len, line_len);
            }
            Action::ScrollLeft if self.would_line_underflow() => {
                self.underflow_line(lines_len, line_len);
            }
            Action::ScrollRight if self.would_line_overflow(lines_len) => {
                self.overflow_line(lines_len, line_len);
            }
            Action::ScrollUp
            | Action::ScrollDown
            | Action::ScrollPageUp
            | Action::ScrollPageDown
            | Action::ScrollTop
            | Action::ScrollBottom => self.move_vertical(action, lines_len, line_len),
            _ => {
                self.scroller.move_cursor(action, lines_len, line_len);
                let vertical = self.scroller.position().vertical;
                let horizontal = self.scroller.horizontal();
                let new_line_len = self.line_len(vertical);
                self.set_cursor(vertical, horizontal.min(new_line_len));
            }
        }
    }

    fn set_cursor(&mut self, vertical: usize, horizontal: usize) {
        self.preferred_horizontal = horizontal;
        self.scroller.set_position(Position {
            vertical,
            horizontal,
        });
    }

    fn move_vertical(&mut self, action: Action, lines_len: usize, line_len: usize) {
        self.scroller.move_cursor(action, lines_len, line_len);
        let vertical = self.scroller.vertical();
        let horizontal = self.preferred_horizontal.min(self.line_len(vertical));
        self.scroller.set_position(Position {
            vertical,
            horizontal,
        });
    }

    fn scroll_word_right(&mut self, lines_len: usize, line_len: usize) {
        let position = self.scroller.position();
        let chars = self.line_chars(position.vertical);
        let target = word_right(&chars, position.horizontal);
        if target > position.horizontal {
            self.set_cursor(position.vertical, target);
        } else if position.vertical + 1 < lines_len {
            self.scroller
                .move_cursor(Action::ScrollDown, lines_len, line_len);
            let next = self.scroller.vertical();
            self.set_cursor(next, 0);
        }
    }

    fn scroll_word_left(&mut self, lines_len: usize, line_len: usize) {
        let position = self.scroller.position();
        let chars = self.line_chars(position.vertical);
        let target = word_left(&chars, position.horizontal);
        if target < position.horizontal {
            self.set_cursor(position.vertical, target);
        } else if position.vertical > 0 {
            self.scroller
                .move_cursor(Action::ScrollUp, lines_len, line_len);
            let prev = self.scroller.vertical();
            self.set_cursor(prev, self.line_len(prev));
        }
    }

    fn underflow_line(&mut self, lines_len: usize, line_len: usize) {
        self.scroller
            .move_cursor(Action::ScrollUp, lines_len, line_len);

        let prev = self.scroller.vertical();
        self.set_cursor(prev, self.line_len(prev));
    }

    fn overflow_line(&mut self, lines_len: usize, line_len: usize) {
        self.scroller
            .move_cursor(Action::ScrollDown, lines_len, line_len);

        let next = self.scroller.vertical();
        self.set_cursor(next, 0);
    }

    fn would_line_underflow(&self) -> bool {
        let position = self.scroller.position();
        position.horizontal == 0 && position.vertical > 0
    }

    fn would_line_overflow(&self, lines_len: usize) -> bool {
        let position = self.scroller.position();
        let line_len = self.line_len(position.vertical);
        position.horizontal >= line_len && position.vertical + 1 < lines_len
    }

    fn remove_char(&mut self, lines: &mut [String], position: Position<usize>) {
        let Position {
            vertical,
            horizontal,
        } = position;

        let line = &lines[vertical];
        let mut chars: Vec<char> = line.chars().collect();
        chars.remove(horizontal - 1);
        lines[vertical] = chars.into_iter().collect();
        self.set_cursor(vertical, horizontal - 1);
    }

    fn remove_line(&mut self, lines: &mut Vec<String>, position: Position<usize>) {
        let Position { vertical, .. } = position;

        let prev = lines.remove(vertical - 1);
        let current = lines.remove(vertical - 1);
        let new_column = prev.chars().count();
        let new_line = format!("{prev}{current}");
        lines.insert(vertical - 1, new_line);
        self.set_cursor(vertical - 1, new_column);
    }

    fn line_len(&self, vertical: usize) -> usize {
        self.lines().nth(vertical).map_or(0, |l| l.chars().count())
    }

    fn line_chars(&self, vertical: usize) -> Vec<char> {
        self.lines().nth(vertical).unwrap_or("").chars().collect()
    }

    fn lines_vec(&self) -> Vec<String> {
        self.lines().map(String::from).collect()
    }

    fn lines(&self) -> impl Iterator<Item = &str> {
        self.context.split("\n")
    }

    fn gutter_width(&self) -> u16 {
        let line_count = self.lines().count();
        format!("{:<6} ", line_count).chars().count() as u16
    }

    fn get_position(&self, lines_len: usize) -> Position<usize> {
        let cursor = self.scroller.position();
        let last_line = lines_len.saturating_sub(1);
        let vertical = cursor.vertical.min(last_line);
        let horizontal = cursor.horizontal;

        Position {
            vertical,
            horizontal,
        }
    }

    fn insert_in_line<'a>(&self, line: &str, line_num: Span<'a>) -> Vec<Span<'a>> {
        let column = self.scroller.horizontal();
        let chars: Vec<char> = line.chars().collect();

        let (before, cursor_char, after) = if column < chars.len() {
            let before = to_string(&chars[..column]);
            let cursor_char = to_string(&chars[column..=column]);
            let after = to_string(&chars[column + 1..]);

            (before, cursor_char, after)
        } else {
            (line.to_string(), " ".to_string(), String::new())
        };

        vec![
            line_num,
            Span::styled(before, Theme::text_normal()),
            Span::styled(cursor_char, Theme::cursor()),
            Span::styled(after, Theme::text_normal()),
        ]
    }
}

impl Component for Content {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: PanelContext) {
        let focused = context == PanelContext::Editor;

        let title_style = if focused {
            Theme::title_focused()
        } else {
            Theme::title_default()
        };

        let border_style = if focused {
            Theme::border_focused()
        } else {
            Theme::border_default()
        };

        let inner_height = area.height.saturating_sub(2);
        let inner_width = area.width.saturating_sub(2);
        let cursor_visual_line = self.scroller.vertical() as u16;
        let gutter_width = self.gutter_width();
        let scroll_offset =
            self.scroller
                .get_scroll(cursor_visual_line, inner_height, inner_width, gutter_width);

        let mut lines = vec![];
        for (i, line) in self.context.lines().enumerate() {
            let selected = self.scroller.vertical() == i;

            let line_str = format!("{:<6} ", i + 1);
            let line_num = Span::styled(line_str, Theme::line_number());

            let spans = if selected {
                self.insert_in_line(line, line_num)
            } else {
                vec![line_num, Span::styled(line, Theme::text_normal())]
            };

            lines.push(Line::from(spans));
        }

        let block = Block::default()
            .title(Span::styled(" Editor ", title_style))
            .borders(Borders::ALL)
            .border_style(border_style);

        let paragraph = Paragraph::new(lines)
            .block(block)
            .style(Theme::editor_background())
            .scroll((scroll_offset.vertical, scroll_offset.horizontal));

        frame.render_widget(paragraph, area);
    }
}

fn to_string(chars: &[char]) -> String {
    chars.iter().collect()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn word_left(line: &[char], start: usize) -> usize {
    let mut i = start;
    while i > 0 && !is_word_char(line[i - 1]) {
        i -= 1;
    }
    while i > 0 && is_word_char(line[i - 1]) {
        i -= 1;
    }
    i
}

fn word_right(line: &[char], start: usize) -> usize {
    let mut i = start;
    while i < line.len() && !is_word_char(line[i]) {
        i += 1;
    }
    while i < line.len() && is_word_char(line[i]) {
        i += 1;
    }
    i
}

fn chars_split(line: &str, horizontal: usize) -> (String, String) {
    let split_byte = line
        .char_indices()
        .nth(horizontal)
        .map(|(i, _)| i)
        .unwrap_or(line.len());

    let (before, after) = line.split_at(split_byte);
    (before.to_string(), after.to_string())
}
