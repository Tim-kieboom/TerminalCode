use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{StartupArgs, app::components::Component, keybinds::PanelContext, theme::Theme};

pub struct Tabs {}
impl Tabs {
    pub fn new(_args: &StartupArgs) -> Self {
        Self {}
    }
}
impl Component for Tabs {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: PanelContext) {
        let tab_content = Line::from(vec![
            Span::styled(" main.rs ", Theme::tab_active()),
            Span::styled(" lib.rs ", Theme::tab_inactive()),
        ]);

        let border_style = Theme::border_default();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style);

        let paragraph = Paragraph::new(tab_content)
            .block(block)
            .style(Theme::editor_background());
        frame.render_widget(paragraph, area);
    }
}
