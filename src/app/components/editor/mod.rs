pub mod tabs;
pub mod content;
pub mod bottombar;

use ratatui::{Frame, layout::{Constraint, Direction, Layout, Rect}};
use crate::{StartupArgs, app::components::{Component, Hideable, editor::{bottombar::BottomBar, content::Content, tabs::Tabs}}, keybinds::PanelContext};

// 🤚✋
const FUNNY_NUMBER: u16 = 67;

pub struct Editor {
    pub tabs: Tabs,
    pub content: Content,
    pub bottombar: Hideable<BottomBar>,
}
impl Editor {
    pub fn new(args: &StartupArgs) -> Self {
        Self { 
            tabs: Tabs::new(args), 
            content: Content::new(args), 
            bottombar: Hideable::new_hide(BottomBar::new(args)),
        }
    }
}
impl Component for Editor {
    fn draw(&self, frame: &mut Frame, area: Rect, context: PanelContext) {
        let has_bottombar = self.bottombar.should_show();
        
        let constraints = if has_bottombar {
            [Constraint::Length(3), Constraint::Percentage(FUNNY_NUMBER), Constraint::Percentage(32)].as_slice()
        } else {
            [Constraint::Length(3), Constraint::Min(1)].as_slice()
        };

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(area);

        self.tabs.draw(frame, layout[0], context);
        self.content.draw(frame, layout[1], context);

        if has_bottombar {
            self.bottombar.draw(frame, layout[2], context);
        }
    }
}