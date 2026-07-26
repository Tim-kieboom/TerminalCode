use ratatui::{Frame, layout::Rect};
mod explorer;
use crate::{StartupArgs, app::{components::{Component, sidebar::explorer::Explorer}}, keybinds::PanelContext};

pub enum SideBarSelect {
    Explorer
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
}

impl Component for SideBar {
    fn draw(&self, frame: &mut Frame, area: Rect, context: PanelContext) {
        
        match self.select {
            SideBarSelect::Explorer => self.explorer.draw(frame, area, context),
        }
    }
}