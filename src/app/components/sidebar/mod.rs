use std::path::PathBuf;

use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    StartupArgs,
    app::components::{
        Component,
        sidebar::{debugger::Debugger, explorer::Explorer},
    },
    keybinds::{Action, PanelContext},
    layout::sidebar::TABS_HEIGHT,
    theme::Theme,
    utils::vertical_layout,
};

pub mod debugger;
pub mod explorer;
pub mod file_tree;

#[cfg(test)]
#[path = "sidebar_tests.rs"]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideBarSelect {
    Explorer,
    Debugger,
}
impl SideBarSelect {
    pub fn all() -> [Self; 2] {
        [Self::Explorer, Self::Debugger]
    }

    fn index(&self) -> usize {
        match self {
            Self::Explorer => 0,
            Self::Debugger => 1,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Explorer => "Explorer",
            Self::Debugger => "Debugger",
        }
    }
}

pub struct SideBar {
    pub select: SideBarSelect,
    pub explorer: Explorer,
    pub debugger: Debugger,
}
impl SideBar {
    pub fn new(args: &StartupArgs) -> Self {
        Self {
            select: SideBarSelect::Explorer,
            explorer: Explorer::new(args),
            debugger: Debugger::new(args),
        }
    }

    pub fn switch_tab(&mut self, amount: isize) {
        let tabs = SideBarSelect::all();
        let len = tabs.len() as isize;
        let index = self.select.index() as isize + amount;
        self.select = tabs[index.rem_euclid(len) as usize];
    }

    pub fn move_cursor(&mut self, action: Action) {
        match self.select {
            SideBarSelect::Explorer => self.explorer.move_cursor(action),
            SideBarSelect::Debugger => self.debugger.move_cursor(action),
        }
    }

    pub fn open_current(&mut self) -> Option<PathBuf> {
        match self.select {
            SideBarSelect::Explorer => self.explorer.open_current(),
            SideBarSelect::Debugger => None,
        }
    }
}

impl Component for SideBar {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: PanelContext) {
        let layout = vertical_layout([TABS_HEIGHT, Constraint::Min(1)], area);
        self.draw_tabs(frame, layout[0]);

        match self.select {
            SideBarSelect::Explorer => self.explorer.draw(frame, layout[1], context),
            SideBarSelect::Debugger => self.debugger.draw(frame, layout[1], context),
        }
    }
}

impl SideBar {
    fn draw_tabs(&self, frame: &mut Frame, area: Rect) {
        let spans: Vec<Span> = SideBarSelect::all()
            .iter()
            .map(|select| {
                let active = *select == self.select;
                let style = if active {
                    Theme::tab_active()
                } else {
                    Theme::tab_inactive()
                };
                Span::styled(format!(" {} ", select.label()), style)
            })
            .collect();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border_default());

        let paragraph = Paragraph::new(Line::from(spans))
            .block(block)
            .style(Theme::editor_background());

        frame.render_widget(paragraph, area);
    }
}
