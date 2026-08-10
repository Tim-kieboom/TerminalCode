use crate::{
    StartupArgs,
    app::components::{Component, editor::bottombar::terminal::Terminal},
    keybinds::PanelContext,
};
use ratatui::{Frame, layout::Rect};
mod terminal;
pub mod terminal_keys;

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

    pub(crate) fn take_errors(&mut self) -> Vec<String> {
        self.terminal.take_errors()
    }

    pub(crate) fn write_input(&mut self, bytes: &[u8]) {
        self.terminal.write_input(bytes);
    }

    pub(crate) fn scroll(&mut self, amount: i16) {
        self.terminal.scroll(amount);
    }
}
impl Component for BottomBar {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: PanelContext) {
        match self.select {
            BottomBarSelect::Terminal => self.terminal.draw(frame, area, context),
        }
    }
}
