use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{StartupArgs, app::components::{Component, utils::cursor_scroller::{CursorScroller, ScrollMode}}, keybinds::{Action, PanelContext}, theme::Theme};

const TEXT: &str = r#"
    fn main() {
        println!("hello world");
    }

    pub struct Content {
        context: String,
    }

    impl Content {
        pub fn new(_args: &StartupArgs) -> Self {
            Self {
                context: TEXT.to_string(),
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

            let mut lines = vec![];
            for (i, line) in self.context.lines().enumerate() {
                let i = i+1;
                lines.push(Line::from(vec![
                    Span::styled(format!("   {i} "), Theme::line_number()),
                    Span::styled(line, Theme::text_accent())
                ]));
            }

            let block = Block::default()
                .title(Span::styled(" Editor ", title_style))
                .borders(Borders::ALL)
                .border_style(border_style);

            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(paragraph, area);
        }
    }
"#;

pub struct Content {
    context: String,
    scroller: CursorScroller,
}

impl Content {
    pub fn new(_args: &StartupArgs) -> Self {
        Self {
            context: TEXT.to_string(),
            scroller: CursorScroller::new(ScrollMode::TextEditor),
        }
    }

    pub fn move_curser(&mut self, action: Action) {
        let line_len = self.context.lines().nth(self.scroller.cursor().vertical).map_or(0, |l| l.len());
        self.scroller.move_cursor(action, self.lines_len(), line_len);
        if let Some(line) = self.context.lines().nth(self.scroller.cursor().vertical) {
            self.scroller.clamp_col(line.len());
        }
    }

    fn lines_len(&self) -> usize {
        self.context.lines().count()
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
            let marker = if selected { " >" } else { "  " };
            let line_num = Span::styled(format!("{} {} ", marker, i+1), Theme::line_number());

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
