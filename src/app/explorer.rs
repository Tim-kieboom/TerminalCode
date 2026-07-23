use std::vec;

use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{App, SelectedPanel};

impl App {
    pub(super) fn draw_explorer(&self, frame: &mut Frame, area: Rect) {
        let focused = self.selected_panel == SelectedPanel::Explorer;

        let title = if focused {
            " Explorer [focused] "
        } else {
            " Explorer "
        };

        let workspace_name = self
            .args
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("workspace");

        let content = vec![
            Line::from(format!("▼ {workspace_name}")),
            Line::from("  src"),
            Line::from("    main.rs"),
            Line::from("    lib.rs"),
            Line::from("    app.rs"),
        ];

        let explorer =
            Paragraph::new(content).block(Block::default().title(title).borders(Borders::ALL));

        frame.render_widget(explorer, area);
    }
}
