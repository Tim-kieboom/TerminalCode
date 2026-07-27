use std::{format, vec};

use ratatui::{Frame, layout::Rect, text::{Line, Span}, widgets::{Block, Borders, Clear, Paragraph}};

use crate::{StartupArgs, app::components::Component, keybinds::PanelContext, theme::Theme, utils::popup_layout};

const MIN_WIDTH: u16 = 100;

#[derive(Debug)]
pub enum DebugTag {
    Note,
    Error,
    Warning,
}

pub struct DebugWindow {
    cursor: usize,
    messages: Vec<DebugMessage>,
}
impl DebugWindow {
    pub fn new(arg: &StartupArgs) -> Self {
        let mut this = Self {
            cursor: 0,
            messages: vec![],
        };

        this.push_note(format!("{arg:?}"));
        this.push_error(format!("1"));
        this.push_warning(format!("2"));
        this.push_note(format!("3"));
        this.push_error(format!("4"));
        this.push_warning(format!("5"));
        this
    }

    fn cursor_line(&self, lines_count: u16) -> u16 {
        if (self.cursor as u16) < lines_count {
            2 + self.cursor as u16
        } else {
            4 + self.cursor as u16
        }
    }

    fn scroll_offset(cursor_line: u16, inner_height: u16) -> u16 {
        if cursor_line < inner_height {
            0
        } else {
            cursor_line - inner_height + 1
        }
    }

    pub fn push_note(&mut self, message: String) {
        self.messages.push(DebugMessage{
            message,
            tag: DebugTag::Note,
        });
    }

    pub fn push_error(&mut self, message: String) {
        self.messages.push(DebugMessage{
            message,
            tag: DebugTag::Error,
        });
    }

    pub fn push_warning(&mut self, message: String) {
        self.messages.push(DebugMessage{
            message,
            tag: DebugTag::Warning,
        });
    }
}
impl Component for DebugWindow {
    fn draw(&self, frame: &mut Frame, area: Rect, _context: PanelContext) {
        let lines_len = self.messages.len() as u16;
        
        let popup_width = MIN_WIDTH.min(area.width.saturating_sub(4));

        let length = lines_len + 6;
        let popup_height = length.min(area.height.saturating_sub(2));
        let inner_height = popup_height.saturating_sub(2);
        let layout = popup_layout(area, popup_width, popup_height);

        let popup_area = layout[1];
        frame.render_widget(Clear, popup_area);

        let cursor_line = self.cursor_line(lines_len);
        let offset = Self::scroll_offset(cursor_line, inner_height);

        let mut lines = vec![Line::from("")];
        for (i, message) in self.messages.iter().enumerate() {
            
            let mut style = match message.tag {
                DebugTag::Note => Theme::text_note(),
                DebugTag::Error => Theme::text_error(),
                DebugTag::Warning => Theme::text_warning(),
            };

            let selected = i == self.cursor;
            if selected {
                Theme::add_highlight(&mut style);
            }

            let text = format!("[{:?}]  {}{}", message.tag, message.format_space(), message.as_str());
            lines.push(Line::styled(text, style));
        }

        let block = Block::default()
            .title(Span::styled(" Debug Window ", Theme::popup_title()))
            .borders(Borders::ALL)
            .border_style(Theme::popup_border());

        let paragraph = Paragraph::new(lines)
            .block(block)
            .scroll((offset, 0));

        frame.render_widget(paragraph, popup_area);
    }
}

pub struct DebugMessage {
    pub tag: DebugTag,
    pub message: String,
}
impl DebugMessage {
    pub fn as_str(&self) -> &str {
        &self.message
    }

    pub fn format_space(&self) -> &str {
        match self.tag {
            DebugTag::Note => "   ",
            DebugTag::Error => "  ",
            DebugTag::Warning => "",
        }
    }
}