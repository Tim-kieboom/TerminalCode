pub mod tabs;
pub mod content;
pub mod bottombar;

use ratatui::{Frame, layout::{Constraint, Rect}};
use crate::{StartupArgs, app::components::{Component, Hideable, editor::{bottombar::BottomBar, content::Content, tabs::Tabs}}, keybinds::PanelContext, utils::vertical_layout};

const TABS_HEIGHT: Constraint = Constraint::Length(3);
const CONTENT_HEIGHT: Constraint = Constraint::Percentage(68);
const BOTTOMBAR_HEIGHT: Constraint = Constraint::Percentage(32);

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
            [TABS_HEIGHT, CONTENT_HEIGHT, BOTTOMBAR_HEIGHT].as_slice()
        } else {
            [TABS_HEIGHT, Constraint::Min(1)].as_slice()
        };

        let layout = vertical_layout(constraints, area);
        self.tabs.draw(frame, layout[0], context);
        self.content.draw(frame, layout[1], context);

        if has_bottombar {
            self.bottombar.try_draw(frame, layout[2], context);
        }
    }
}