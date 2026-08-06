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

pub struct Content {
    context: String,
    scroller: CursorScroller,
}

impl Content {
    pub fn new(_args: &StartupArgs) -> Self {
        Self {
            context: "".to_string(),
            scroller: CursorScroller::new(ScrollMode::TextEditor),
        }
    }

    pub fn open(&mut self, path: &Path) -> std::io::Result<()> {
        let text = fs::read_to_string(path)?;
        self.context = text.replace("\r\n", "\n");
        self.scroller.set_cursor(Position::default());
        Ok(())
    }

    pub fn move_curser(&mut self, action: Action) {
        let vertical = self.scroller.cursor().vertical;

        let lines_len = self.lines().count();
        let line = self.lines().nth(vertical);

        let line_len = line.map_or(0, |l| l.len());
        self.scroller.move_cursor(action, lines_len, line_len);
        self.scroller.clamp_column(line_len);
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
        self.scroller.set_cursor(Position {
            vertical,
            horizontal: posistion + 1,
        });
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
        self.scroller.set_cursor(Position {
            vertical,
            horizontal: clamped,
        });
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
        self.scroller.set_cursor(Position {
            vertical: vertical + 1,
            horizontal: 0,
        });
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

    fn lines_vec(&self) -> Vec<String> {
        self.lines().map(String::from).collect()
    }

    fn lines(&self) -> impl Iterator<Item = &str> {
        self.context.split("\n")
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
        let column = self.scroller.cursor().horizontal;
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
        let cursor_visual_line = self.scroller.cursor().vertical as u16;
        let scroll_offset = self
            .scroller
            .get_scroll(cursor_visual_line, inner_height, inner_width);

        let mut lines = vec![];
        for (i, line) in self.context.lines().enumerate() {
            let selected = self.scroller.cursor().vertical == i;

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
