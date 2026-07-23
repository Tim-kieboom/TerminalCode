use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{App, SelectedPanel, theme::Theme};

impl App {
    pub(super) fn draw_editor(&self, frame: &mut Frame, area: Rect) {
        let editor_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(area);

        self.draw_tabs(frame, editor_layout[0]);
        self.draw_editor_content(frame, editor_layout[1]);
    }

    fn draw_tabs(&self, frame: &mut Frame, area: Rect) {
        let tab_content = Line::from(vec![
            Span::styled(" main.rs ", Theme::tab_active()),
            Span::styled(" lib.rs ", Theme::tab_inactive()),
        ]);

        let border_style = Theme::border_default();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style);

        let paragraph = Paragraph::new(tab_content).block(block);
        frame.render_widget(paragraph, area);
    }

    fn draw_editor_content(&self, frame: &mut Frame, area: Rect) {
        let focused = self.selected_panel == SelectedPanel::Editor;

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

        let title = if focused { " EDITOR " } else { " Editor " };

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
            .title(Span::styled(title, title_style))
            .borders(Borders::ALL)
            .border_style(border_style);

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
    }
}
