use crate::{
    StartupArgs,
    app::components::{Component, editor::bottombar::terminal::Terminal},
    keybinds::PanelContext,
};
use ratatui::{Frame, layout::Rect};
mod terminal;

pub enum BottomBarSelect {
    Terminal,
}

pub struct BottomBar {
    terminal: Terminal,
    select: BottomBarSelect,
}
impl BottomBar {
    pub fn new(args: &StartupArgs) -> Self {
        Self {
            terminal: Terminal::new(args),
            select: BottomBarSelect::Terminal,
        }
    }
}
impl Component for BottomBar {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: PanelContext) {
        match self.select {
            BottomBarSelect::Terminal => self.terminal.draw(frame, area, context),
        }
    }
}
