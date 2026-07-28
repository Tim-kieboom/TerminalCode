use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{StartupArgs, app::components::{Component, utils::cursor_scroller::{CursorScroller, Position, ScrollMode}}, keybinds::{Action, PanelContext}, theme::Theme};

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

    pub fn move_curser(&mut self, action: Action) {
        let lines: Vec<&str> = self.context.split('\n').collect();
        let line_len = lines.get(self.scroller.cursor().vertical).map_or(0, |l| l.len());
        self.scroller.move_cursor(action, lines.len(), line_len);
        if let Some(line) = lines.get(self.scroller.cursor().vertical) {
            self.scroller.clamp_col(line.len());
        }
    }

    pub fn insert_char(&mut self, c: char) {
        let mut lines: Vec<String> = self.context.split('\n').map(String::from).collect();
        let v = self.scroller.cursor().vertical.min(lines.len().saturating_sub(1));
        let h = self.scroller.cursor().horizontal;

        let line = &mut lines[v];
        let mut chars: Vec<char> = line.chars().collect();
        let pos = h.min(chars.len());
        chars.insert(pos, c);
        *line = chars.into_iter().collect();

        self.context = lines.join("\n");
        self.scroller.set_cursor(Position { vertical: v, horizontal: pos + 1 });
    }

    pub fn backspace(&mut self) {
        let mut lines: Vec<String> = self.context.split('\n').map(String::from).collect();
        let v = self.scroller.cursor().vertical.min(lines.len().saturating_sub(1));
        let h = self.scroller.cursor().horizontal;

        if h > 0 {
            let line = &lines[v];
            let mut chars: Vec<char> = line.chars().collect();
            chars.remove(h - 1);
            lines[v] = chars.into_iter().collect();
            self.scroller.set_cursor(Position { vertical: v, horizontal: h - 1 });
        } else if v > 0 {
            let prev = lines.remove(v - 1);
            let cur = lines.remove(v - 1);
            let new_col = prev.chars().count();
            let new_line = format!("{prev}{cur}");
            lines.insert(v - 1, new_line);
            self.scroller.set_cursor(Position { vertical: v - 1, horizontal: new_col });
        }

        self.context = lines.join("\n");
    }

    pub fn delete_char(&mut self) {
        let mut lines: Vec<String> = self.context.split('\n').map(String::from).collect();
        let v = self.scroller.cursor().vertical.min(lines.len().saturating_sub(1));
        let h = self.scroller.cursor().horizontal;

        if h < lines[v].chars().count() {
            let mut chars: Vec<char> = lines[v].chars().collect();
            chars.remove(h);
            lines[v] = chars.into_iter().collect();
        } else if v + 1 < lines.len() {
            let next = lines.remove(v + 1);
            lines[v] = format!("{}{}", lines[v], next);
        }

        self.context = lines.join("\n");
        let clamped = h.min(lines[v].chars().count());
        self.scroller.set_cursor(Position { vertical: v, horizontal: clamped });
    }

    pub fn insert_newline(&mut self) {
        let mut lines: Vec<String> = self.context.split('\n').map(String::from).collect();
        let v = self.scroller.cursor().vertical.min(lines.len().saturating_sub(1));
        let h = self.scroller.cursor().horizontal;

        let chars: Vec<char> = lines[v].chars().collect();
        let pos = h.min(chars.len());
        let after: String = chars[pos..].iter().collect();
        lines[v] = chars[..pos].iter().collect();

        lines.insert(v + 1, after);

        self.context = lines.join("\n");
        self.scroller.set_cursor(Position { vertical: v + 1, horizontal: 0 });
    }

    pub fn insert_tab(&mut self) {
        for _ in 0..4 {
            self.insert_char(' ');
        }
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
        let scroll_offset = self.scroller.get_scroll(cursor_visual_line, inner_height, inner_width);

        let mut lines = vec![];
        for (i, line) in self.context.lines().enumerate() {
            let selected = self.scroller.cursor().vertical == i;
            
            let line_str = format!("{:<5} ", i+1);
            let line_num = Span::styled(line_str, Theme::line_number());

            let spans = if selected {
                let col = self.scroller.cursor().horizontal;
                let chars: Vec<char> = line.chars().collect();
                let (before, cursor_char, after) = if col < chars.len() {
                    let before: String = chars[..col].iter().collect();
                    let c: String = chars[col..=col].iter().collect();
                    let after: String = chars[col+1..].iter().collect();
                    (before, c, after)
                } else {
                    (line.to_string(), " ".to_string(), String::new())
                };

                vec![
                    line_num,
                    Span::styled(before, Theme::text_accent()),
                    Span::styled(cursor_char, Theme::cursor()),
                    Span::styled(after, Theme::text_accent()),
                ]
            } else {
                vec![
                    line_num,
                    Span::styled(line, Theme::text_accent()),
                ]
            };

            lines.push(Line::from(spans));
        }

        let block = Block::default()
            .title(Span::styled(" Editor ", title_style))
            .borders(Borders::ALL)
            .border_style(border_style);

        let paragraph = Paragraph::new(lines)
            .block(block)
            .scroll((scroll_offset.vertical, scroll_offset.horizontal));
        
        frame.render_widget(paragraph, area);
    }
}
