use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{App, SelectedPanel, theme::Theme};

impl App {
    pub(super) fn draw_explorer(&self, frame: &mut Frame, area: Rect) {
        let focused = self.selected_panel == SelectedPanel::Explorer;

        let workspace_name = self
            .args
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("workspace");

        let mut lines: Vec<Line> = Vec::new();

        // Workspace root
        lines.push(Line::from(vec![
            Span::styled("  ", Theme::text_dim()),
            Span::styled("▼ ", Theme::text_accent()),
            Span::styled(workspace_name, Theme::explorer_folder()),
        ]));

        // Directories
        lines.push(Line::from(vec![
            Span::styled("    ", Theme::text_dim()),
            Span::styled("📁 src", Theme::explorer_folder()),
        ]));

        // Files inside src
        let files = ["main.rs", "lib.rs", "app/mod.rs", "terminal.rs"];
        for file in &files {
            lines.push(Line::from(vec![
                Span::styled("      ", Theme::text_dim()),
                Span::styled(*file, Theme::explorer_file()),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("    ", Theme::text_dim()),
            Span::styled("📁 keybinds", Theme::explorer_folder()),
        ]));
        let kb_files = ["mod.rs", "action.rs", "keybinding.rs"];
        for file in &kb_files {
            lines.push(Line::from(vec![
                Span::styled("      ", Theme::text_dim()),
                Span::styled(*file, Theme::explorer_file()),
            ]));
        }

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
            " EXPLORER "
        } else {
            " Explorer "
        };

        let block = Block::default()
            .title(Span::styled(title, title_style))
            .borders(Borders::ALL)
            .border_style(border_style);

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
    }
}
