use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use ropey::{Rope, RopeSlice};

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
    pub(super) content: Option<Rope>,
    pub(super) scroller: CursorScroller,
}

impl Content {
    pub fn new(_args: &StartupArgs) -> Self {
        Self {
            content: None,
            scroller: CursorScroller::new(ScrollMode::TextEditor),
        }
    }

    pub fn load(&mut self, content: impl Into<Rope>) {
        self.content = Some(content.into());
        self.scroller.set_cursor(Position::default());
    }

    pub(super) fn take_content(&mut self) -> Option<Rope> {
        self.content.take()
    }

    pub fn text(&self) -> Option<&Rope> {
        self.content.as_ref()
    }

    pub fn insert_char(&mut self, ch: char) -> bool {
        let Some(content) = &mut self.content else {
            return false;
        };

        let position = clamped_position(&self.scroller, content.len_lines());
        let index = char_index(content, position);
        content.insert_char(index, ch);

        self.scroller.set_cursor(Position {
            vertical: position.vertical,
            horizontal: position.horizontal + 1,
        });

        true
    }

    pub fn delete_char(&mut self) -> bool {
        let Some(content) = &mut self.content else {
            return false;
        };

        let position = clamped_position(&self.scroller, content.len_lines());
        let line_start = content.line_to_char(position.vertical);
        let line_len = line_visual_len(content.line(position.vertical));

        let modified = if position.horizontal < line_len {
            let cursor_index = line_start + position.horizontal;
            content.remove(cursor_index..cursor_index + 1);
            true
        } else if position.vertical + 1 < content.len_lines() {
            let curser_index = line_start + line_len;
            content.remove(curser_index..curser_index + 1);
            true
        } else {
            false
        };

        let clamped = position
            .horizontal
            .min(line_visual_len(content.line(position.vertical)));

        self.scroller.set_cursor(Position {
            vertical: position.vertical,
            horizontal: clamped,
        });

        modified
    }

    pub fn insert_newline(&mut self) -> bool {
        let Some(content) = &mut self.content else {
            return false;
        };

        let position = clamped_position(&self.scroller, content.len_lines());
        let index = char_index(content, position);
        content.insert_char(index, '\n');

        self.scroller.set_cursor(Position {
            vertical: position.vertical + 1,
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
        let Some(content) = &mut self.content else {
            return false;
        };

        let position = clamped_position(&self.scroller, content.len_lines());

        if position.horizontal > 0 {
            let index = content.line_to_char(position.vertical) + position.horizontal - 1;
            content.remove(index..index + 1);
            self.scroller.set_cursor(Position {
                vertical: position.vertical,
                horizontal: position.horizontal - 1,
            });
            true
        } else if position.vertical > 0 {
            let prev_len = line_visual_len(content.line(position.vertical - 1));
            let index = content.line_to_char(position.vertical) - 1;
            content.remove(index..index + 1);
            self.scroller.set_cursor(Position {
                vertical: position.vertical - 1,
                horizontal: prev_len,
            });
            true
        } else {
            false
        }
    }

    pub fn move_cursor(&mut self, action: Action) {
        let Some(content) = &self.content else { return };

        let scroller = &mut self.scroller;

        let lines_len = content.len_lines();
        scroller.move_editor_cursor(
            action,
            lines_len,
            |v| line_visual_len(content.line(v)),
            |v| {
                let line = content.line(v);
                line.chars().take(line_visual_len(line)).collect()
            },
        );
    }

    fn gutter_width(&self) -> u16 {
        let Some(content) = &self.content else {
            return 0;
        };

        let line_count = content.lines().count();
        format!("{:<6} ", line_count).chars().count() as u16
    }

    fn insert_in_line<'a>(&self, line: RopeSlice<'a>, line_num: Span<'a>) -> Vec<Span<'a>> {
        let column = self.scroller.horizontal();
        let chars: Vec<char> = line.chars().take(line_visual_len(line)).collect();

        let (before, cursor_char, after) = if column < chars.len() {
            let before = to_string(&chars[..column]);
            let cursor_char = to_string(&chars[column..=column]);
            let after = to_string(&chars[column + 1..]);

            (before, cursor_char, after)
        } else {
            (to_string(&chars), " ".to_string(), String::new())
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

fn line_visual_len(line: RopeSlice<'_>) -> usize {
    let len = line.len_chars();
    if len > 0 && line.char(len - 1) == '\n' {
        len - 1
    } else {
        len
    }
}

fn char_index(content: &Rope, position: Position<usize>) -> usize {
    let last_line = content.len_lines().saturating_sub(1);
    let vertical = position.vertical.min(last_line);
    let line_start = content.line_to_char(vertical);
    let visual_len = line_visual_len(content.line(vertical));
    line_start + position.horizontal.min(visual_len)
}

fn clamped_position(scroller: &CursorScroller, line_count: usize) -> Position<usize> {
    let cursor = scroller.cursor();
    let last_line = line_count.saturating_sub(1);

    Position {
        vertical: cursor.vertical.min(last_line),
        horizontal: cursor.horizontal,
    }
}
