use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{StartupArgs, app::components::Component, keybinds::PanelContext, theme::Theme};

#[cfg(test)]
#[path = "tests/tabs_tests.rs"]
mod tests;

pub struct Tabs {
    pub(super) files: Vec<String>,
    pub(super) active: usize,
}
impl Tabs {
    pub fn new(_args: &StartupArgs) -> Self {
        Self {
            files: Vec::new(),
            active: 0,
        }
    }

    pub fn open(&mut self, name: impl Into<String>) {
        let name = name.into();
        if let Some(index) = self.files.iter().position(|file| file == &name) {
            self.active = index;
        } else {
            self.files.push(name);
            self.active = self.files.len() - 1;
        }
    }
}
impl Component for Tabs {
    fn draw(&mut self, frame: &mut Frame, area: Rect, _context: PanelContext) {
        let spans: Vec<Span> = self
            .files
            .iter()
            .enumerate()
            .map(|(i, file)| {
                let style = if i == self.active {
                    Theme::tab_active()
                } else {
                    Theme::tab_inactive()
                };
                Span::styled(format!(" {file} "), style)
            })
            .collect();

        let tab_content = Line::from(spans);

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
