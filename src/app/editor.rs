use std::vec;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{App, SelectedPanel};

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
        let tabs = Paragraph::new(" main.rs ").block(Block::default().borders(Borders::ALL));

        frame.render_widget(tabs, area);
    }

    fn draw_editor_content(&self, frame: &mut Frame, area: Rect) {
        let focused = self.selected_panel == SelectedPanel::Editor;

        let title = if focused {
            " Editor [focused] "
        } else {
            " Editor "
        };

        let content = vec![
            Line::from(" 1  fn main() {"),
            Line::from(" 2      println!(\"Hello, terminal editor!\");"),
            Line::from(" 3  }"),
        ];

        let editor =
            Paragraph::new(content).block(Block::default().title(title).borders(Borders::ALL));

        frame.render_widget(editor, area);
    }
}
