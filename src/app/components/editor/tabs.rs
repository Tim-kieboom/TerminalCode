use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    StartupArgs,
    app::components::{Component, editor::file_content::FileContent},
    keybinds::PanelContext,
    theme::Theme,
};

#[cfg(test)]
#[path = "tests/tabs_tests.rs"]
mod tests;

pub struct Tabs {
    pub(super) files: Vec<FileContent>,
    pub(super) active: usize,
}
impl Tabs {
    pub fn new(_args: &StartupArgs) -> Self {
        Self {
            files: Vec::new(),
            active: 0,
        }
    }

    pub(super) fn switch_tab(&mut self, amount: isize) {
        if self.files.is_empty() {
            return;
        }

        let len = self.files.len();
        let index = self.active as isize + amount;
        self.active = index.rem_euclid(len as isize) as usize;
    }

    pub fn active(&self) -> Option<&FileContent> {
        self.files.get(self.active)
    }

    pub fn active_mut(&mut self) -> Option<&mut FileContent> {
        self.files.get_mut(self.active)
    }

    pub fn open(&mut self, file: FileContent) {
        let name = file.name().to_string();
        if let Some(index) = self.files.iter().position(|f| f.name() == name) {
            self.files[index] = file;
            self.active = index;
        } else {
            self.files.push(file);
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
                let label = if file.is_dirty() {
                    format!(" *{} ", file.name())
                } else {
                    format!(" {} ", file.name())
                };
                Span::styled(label, style)
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
