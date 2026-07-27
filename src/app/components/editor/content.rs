use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{StartupArgs, app::components::Component, keybinds::PanelContext, theme::Theme};

pub struct Content {}
impl Content {
    pub fn new(_args: &StartupArgs) -> Self {
        Self {}
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

        let lines = vec![
            Line::from(vec![
                Span::styled("   1 ", Theme::line_number()),
                Span::styled("fn ", Theme::text_accent()),
                Span::styled("main", Theme::text_normal()),
                Span::styled("() {", Theme::text_dim()),
            ]),
            Line::from(vec![
                Span::styled("   2 ", Theme::line_number()),
                Span::styled("    ", Theme::text_dim()),
                Span::styled("println!", Theme::text_accent()),
                Span::styled("(", Theme::text_dim()),
                Span::styled("\"Hello, terminal editor!\"", Theme::text_success()),
                Span::styled(");", Theme::text_dim()),
            ]),
            Line::from(vec![
                Span::styled("   3 ", Theme::line_number()),
                Span::styled("}", Theme::text_dim()),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("   4 ", Theme::line_number()),
                Span::styled("fn ", Theme::text_accent()),
                Span::styled("draw", Theme::text_normal()),
                Span::styled("() {", Theme::text_dim()),
            ]),
            Line::from(vec![
                Span::styled("   5 ", Theme::line_number()),
                Span::styled("    ", Theme::text_dim()),
                Span::styled("// TODO: implement rendering", Theme::text_dim()),
            ]),
            Line::from(vec![
                Span::styled("   6 ", Theme::line_number()),
                Span::styled("}", Theme::text_dim()),
            ]),
        ];

        let block = Block::default()
            .title(Span::styled(" Editor ", title_style))
            .borders(Borders::ALL)
            .border_style(border_style);

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
    }
}
