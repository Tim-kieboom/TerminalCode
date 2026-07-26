use ratatui::{Frame, layout::Rect, text::Span, widgets::{Block, Borders, Paragraph}};

use crate::{StartupArgs, app::{components::Component, theme::Theme}, keybinds::PanelContext};

pub struct BottomBar {}
impl BottomBar {
    pub fn new(_args: &StartupArgs) -> Self {
        Self{}
    }
}
impl Component for BottomBar {
    fn draw(&self, frame: &mut Frame, area: Rect, context: PanelContext) {
        let focused = context == PanelContext::BottomBar;
        
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

        let title = if focused {
            " TERMINAL "
        } else {
            " Terminal "
        };

        let block = Block::default()
            .title(Span::styled(title, title_style))
            .borders(Borders::ALL)
            .border_style(border_style);
        
        let bar = Paragraph::new("").style(Theme::status_bar()).block(block);
        frame.render_widget(bar, area);
    }
}