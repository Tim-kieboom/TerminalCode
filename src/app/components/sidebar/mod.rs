use std::path::PathBuf;

use ratatui::{Frame, layout::Rect};

pub mod explorer;
pub mod file_tree;

use crate::{
    StartupArgs,
    app::components::{Component, sidebar::explorer::Explorer},
    keybinds::{Action, PanelContext},
};

pub enum SideBarSelect {
    Explorer,
}

pub struct SideBar {
    pub select: SideBarSelect,
    pub explorer: Explorer,
}
impl SideBar {
    pub fn new(args: &StartupArgs) -> Self {
        Self {
            select: SideBarSelect::Explorer,
            explorer: Explorer::new(args),
        }
    }

    pub fn move_cursor(&mut self, action: Action) {
        match self.select {
            SideBarSelect::Explorer => self.explorer.move_cursor(action),
        }
    }

    pub fn open_current(&mut self) -> Option<PathBuf> {
        match self.select {
            SideBarSelect::Explorer => self.explorer.open_current(),
        }
    }
}

impl Component for SideBar {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: PanelContext) {
        match self.select {
            SideBarSelect::Explorer => self.explorer.draw(frame, area, context),
        }
    }
}
