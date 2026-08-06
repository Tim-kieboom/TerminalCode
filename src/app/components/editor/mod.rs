pub mod bottombar;
pub mod content;
pub mod tabs;

use std::path::Path;

pub use crate::layout::editor::*;
use crate::{
    StartupArgs,
    app::components::{
        Component, Hideable,
        editor::{bottombar::BottomBar, content::Content, tabs::Tabs},
    },
    keybinds::PanelContext,
    utils::vertical_layout,
};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
};

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

    pub fn open(&mut self, path: &Path) -> std::io::Result<()> {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("<file>")
            .to_string();

        self.content.open(path)?;
        self.tabs.open(name);
        Ok(())
    }
}
impl Component for Editor {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: PanelContext) {
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
