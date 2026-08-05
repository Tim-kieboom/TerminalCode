use std::{format, vec};

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::{
    StartupArgs,
    app::components::{
        Component,
        utils::cursor_scroller::{CursorScroller, ScrollMode},
    },
    keybinds::{Action, PanelContext},
    theme::Theme,
    utils::popup_layout,
};

const MIN_WIDTH: u16 = 100;

#[derive(Debug)]
pub enum DebugTag {
    Note,
    Error,
    Warning,
}

pub struct DebugWindow {
    scroller: CursorScroller,
    messages: Vec<DebugMessage>,
}
impl DebugWindow {
    pub fn new(_arg: &StartupArgs) -> Self {
        Self {
            messages: vec![],
            scroller: CursorScroller::new(ScrollMode::List),
        }
    }

    pub fn push_note(&mut self, message: String) {
        self.messages.push(DebugMessage {
            message,
            tag: DebugTag::Note,
        });
    }

    pub fn push_error(&mut self, message: String) {
        self.messages.push(DebugMessage {
            message,
            tag: DebugTag::Error,
        });
    }

    pub fn push_warning(&mut self, message: String) {
        self.messages.push(DebugMessage {
            message,
            tag: DebugTag::Warning,
        });
    }

    pub fn move_cursor(&mut self, action: Action) {
        let length = self.messages.len();
        self.scroller.move_cursor(action, length, 0);
    }
}
impl Component for DebugWindow {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: PanelContext) {
        let lines_len = self.messages.len() as u16;

        let popup_width = MIN_WIDTH.min(area.width.saturating_sub(4));

        let length = lines_len + 6;
        let popup_height = length.min(area.height.saturating_sub(2));
        let inner_height = popup_height.saturating_sub(2);
        let popup_area = popup_layout(area, popup_width, popup_height);

        frame.render_widget(Clear, popup_area);

        let cursor_visual_line = 1 + self.scroller.cursor().vertical as u16;
        let scroll_offset = self
            .scroller
            .get_scroll(cursor_visual_line, inner_height, 0);

        let mut lines = vec![Line::from("")];
        for (i, message) in self.messages.iter().enumerate() {
            let mut style = match message.tag {
                DebugTag::Note => Theme::text_note(),
                DebugTag::Error => Theme::text_error(),
                DebugTag::Warning => Theme::text_warning(),
            };

            let selected = i == self.scroller.cursor().vertical;
            if selected {
                Theme::add_highlight(&mut style);
            }

            let text = format!(
                "[{:?}]  {}{}",
                message.tag,
                message.format_space(),
                message.as_str(),
            );

            lines.push(Line::styled(text, style));
        }

        let block = Block::default()
            .title(Span::styled(" Debug Window ", Theme::popup_title()))
            .borders(Borders::ALL)
            .border_style(Theme::popup_border());

        let paragraph = Paragraph::new(lines)
            .block(block)
            .scroll((scroll_offset.vertical, scroll_offset.horizontal));

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
