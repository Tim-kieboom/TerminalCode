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
#[path = "tests/content_tests.rs"]
mod tests;

pub struct Content {
    pub(super) content: Option<String>,
    pub(super) scroller: CursorScroller,
}

impl Content {
    pub fn new(_args: &StartupArgs) -> Self {
        Self {
            content: None,
            scroller: CursorScroller::new(ScrollMode::TextEditor),
        }
    }

    pub fn load(&mut self, content: String) {
        self.content = Some(content);
        self.scroller.set_cursor(Position::default());
    }

    pub(super) fn take_content(&mut self) -> Option<String> {
        self.content.take()
    }

    pub fn text(&self) -> Option<&str> {
        self.content.as_deref()
    }

    pub fn insert_char(&mut self, ch: char) -> bool {
        let Some(content) = &self.content else {
            return false;
        };

        let mut lines = content.simple_lines_vec();

        let Position {
            vertical,
            horizontal,
        } = self.get_position(lines.len());

        let line = &mut lines[vertical];
        let mut chars: Vec<char> = line.chars().collect();

        let position = horizontal.min(chars.len());
        chars.insert(position, ch);
        *line = chars.into_iter().collect();

        if let Some(content) = &mut self.content {
            *content = lines.join("\n");
        }

        self.scroller.set_cursor(Position {
            vertical,
            horizontal: position + 1,
        });

        true
    }

    pub fn delete_char(&mut self) -> bool {
        let Some(content) = &self.content else {
            return false;
        };

        let mut lines = content.simple_lines_vec();
        let Position {
            vertical,
            horizontal,
        } = self.get_position(lines.len());

        let mut modified = false;
        if horizontal < lines[vertical].chars().count() {
            let mut chars: Vec<char> = lines[vertical].chars().collect();
            chars.remove(horizontal);
            lines[vertical] = chars.into_iter().collect();
            modified = true;
        } else if vertical + 1 < lines.len() {
            let next = lines.remove(vertical + 1);
            lines[vertical] = format!("{}{}", lines[vertical], next);
            modified = true;
        }

        if let Some(content) = &mut self.content {
            *content = lines.join("\n");
        }

        let clamped = horizontal.min(lines[vertical].chars().count());
        self.scroller.set_cursor(Position {
            vertical,
            horizontal: clamped,
        });

        modified
    }

    pub fn insert_newline(&mut self) -> bool {
        let Some(content) = &self.content else {
            return false;
        };

        let mut lines = content.simple_lines_vec();
        let Position {
            vertical,
            horizontal,
        } = self.get_position(lines.len());

        let (before, after) = chars_split(&lines[vertical], horizontal);
        lines[vertical] = before;
        lines.insert(vertical + 1, after);

        if let Some(content) = &mut self.content {
            *content = lines.join("\n");
        }

        self.scroller.set_cursor(Position {
            vertical: vertical + 1,
            horizontal: 0,
        });

        true
    }

    pub fn insert_tab(&mut self) -> bool {
        let mut modified = false;
        for _ in 0..4 {
            modified |= self.insert_char(' ');
        }
        modified
    }

    pub fn backspace(&mut self) -> bool {
        let Some(content) = &self.content else {
            return false;
        };

        let mut lines = content.simple_lines_vec();
        let position = self.get_position(lines.len());

        let modified = if position.horizontal > 0 {
            self.remove_char(&mut lines, position);
            true
        } else if position.vertical > 0 {
            self.remove_line(&mut lines, position);
            true
        } else {
            false
        };

        if let Some(content) = &mut self.content {
            *content = lines.join("\n");
        }

        modified
    }

    pub fn move_cursor(&mut self, action: Action) {
        let Some(content) = &self.content else { return };

        let scroller = &mut self.scroller;

        let lines_len = content.simple_lines().count();
        scroller.move_editor_cursor(
            action,
            lines_len,
            |v| {
                content
                    .simple_lines()
                    .nth(v)
                    .map_or(0, |l| l.chars().count())
            },
            |v| {
                content
                    .simple_lines()
                    .nth(v)
                    .unwrap_or("")
                    .chars()
                    .collect()
            },
        );
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
        self.scroller.set_cursor(Position {
            vertical,
            horizontal: horizontal - 1,
        });
    }

    fn remove_line(&mut self, lines: &mut Vec<String>, position: Position<usize>) {
        let Position { vertical, .. } = position;

        let prev = lines.remove(vertical - 1);
        let current = lines.remove(vertical - 1);
        let new_column = prev.chars().count();
        let new_line = format!("{prev}{current}");
        lines.insert(vertical - 1, new_line);
        self.scroller.set_cursor(Position {
            vertical: vertical - 1,
            horizontal: new_column,
        });
    }

    fn gutter_width(&self) -> u16 {
        let Some(content) = &self.content else {
            return 0;
        };

        let line_count = content.lines().count();
        format!("{:<6} ", line_count).chars().count() as u16
    }

    fn get_position(&self, lines_len: usize) -> Position<usize> {
        let cursor = self.scroller.cursor();
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
        let Some(content) = &self.content else {
            let block = Block::default()
                .title(Span::styled(" Editor ", title_style))
                .borders(Borders::ALL)
                .border_style(border_style);

            let paragraph = Paragraph::new(lines)
                .block(block)
                .style(Theme::editor_background())
                .scroll((scroll_offset.vertical, scroll_offset.horizontal));

            frame.render_widget(paragraph, area);
            return;
        };

        for (i, line) in content.lines().enumerate() {
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

fn chars_split(line: &str, horizontal: usize) -> (String, String) {
    let split_byte = line
        .char_indices()
        .nth(horizontal)
        .map(|(i, _)| i)
        .unwrap_or(line.len());

    let (before, after) = line.split_at(split_byte);
    (before.to_string(), after.to_string())
}

trait SimpleLines {
    fn simple_lines_vec(&self) -> Vec<String>;
    fn simple_lines(&self) -> impl Iterator<Item = &str>;
}
impl SimpleLines for String {
    fn simple_lines_vec(&self) -> Vec<String> {
        self.simple_lines().map(String::from).collect()
    }

    fn simple_lines(&self) -> impl Iterator<Item = &str> {
        self.split("\n")
    }
}
