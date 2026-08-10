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
        utils::cursor_scroller::{CursorScroller, ScrollMode},
    },
    keybinds::{Action, PanelContext},
    theme::Theme,
};

pub struct Debugger {
    scroller: CursorScroller,
}

impl Debugger {
    pub fn new(_args: &StartupArgs) -> Self {
        Self {
            scroller: CursorScroller::new(ScrollMode::List),
        }
    }

    pub fn move_cursor(&mut self, action: Action) {
        self.scroller.move_cursor(action, 0, 0);
    }
}

impl Component for Debugger {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: PanelContext) {
        let focused = context == PanelContext::SideBar;

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

        let block = Block::default()
            .title(Span::styled(" Debugger ", title_style))
            .borders(Borders::ALL)
            .border_style(border_style);

        let placeholder = vec![
            Line::from(""),
            Line::from(Span::styled("  No active debug session", Theme::text_dim())),
        ];

        let paragraph = Paragraph::new(placeholder)
            .block(block)
            .style(Theme::explorer_bg());

        frame.render_widget(paragraph, area);
    }
}
