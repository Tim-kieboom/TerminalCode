use ratatui::{Frame, layout::Rect, text::{Line, Span}, widgets::{Block, Borders, Paragraph}};
use crate::{StartupArgs, app::{components::Component}, theme::Theme, keybinds::PanelContext};

pub struct Explorer {
    workspace_name: String,
}
impl Explorer {
    pub fn new(args: &StartupArgs) -> Self {

        let name = args
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<null>")
            .to_string();

        Self{ 
            workspace_name: name
        }
    }
}

impl Component for Explorer {
    fn draw(&self, frame: &mut Frame, area: Rect, context: PanelContext) {
        let focused = context == PanelContext::SideBar;
        let mut lines: Vec<Line> = Vec::new();

        let name = &self.workspace_name;
        lines.push(Line::from(vec![
            Span::styled("  ", Theme::text_dim()),
            Span::styled("▼ ", Theme::text_accent()),
            Span::styled(name, Theme::explorer_folder()),
        ]));

        lines.push(Line::from(vec![
            Span::styled("    ", Theme::text_dim()),
            Span::styled("📁 src", Theme::explorer_folder()),
        ]));

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

        let block = Block::default()
            .title(Span::styled(" Explorer ", title_style))
            .borders(Borders::ALL)
            .border_style(border_style);

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
    }
}