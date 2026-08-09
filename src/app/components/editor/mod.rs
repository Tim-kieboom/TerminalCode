pub mod bottombar;
pub mod content;
mod file_content;
pub mod tabs;

#[cfg(test)]
#[path = "tests/editor_tests.rs"]
mod tests;

use std::{fs, io, path::Path};

pub use crate::layout::editor::*;
use crate::{
    StartupArgs,
    app::components::{
        Component, Hideable,
        editor::{bottombar::BottomBar, content::Content, file_content::FileContent, tabs::Tabs},
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

    pub fn open(&mut self, path: &Path) -> io::Result<()> {
        self.commit_active();

        let file = FileContent::read_from_path(path)?;
        self.tabs.open(file);
        self.load_active();
        Ok(())
    }

    pub fn switch_tab(&mut self, amount: isize) {
        if self.tabs.files.is_empty() {
            return;
        }

        self.commit_active();
        self.tabs.switch_tab(amount);
        self.load_active();
    }

    pub fn insert_char(&mut self, ch: char) {
        if self.content.insert_char(ch) {
            self.mark_active_dirty();
        }
    }

    pub fn delete_char(&mut self) {
        if self.content.delete_char() {
            self.mark_active_dirty();
        }
    }

    pub fn insert_newline(&mut self) {
        if self.content.insert_newline() {
            self.mark_active_dirty();
        }
    }

    pub fn insert_tab(&mut self) {
        if self.content.insert_tab() {
            self.mark_active_dirty();
        }
    }

    pub fn backspace(&mut self) {
        if self.content.backspace() {
            self.mark_active_dirty();
        }
    }

    pub fn save_active(&mut self) -> io::Result<bool> {
        let Some(text) = self.content.text() else {
            return Ok(false);
        };
        let Some(file) = self.tabs.active() else {
            return Ok(false);
        };
        if !file.is_dirty() {
            return Ok(false);
        }

        fs::write(file.path(), text)?;

        if let Some(file) = self.tabs.active_mut() {
            *file.content_mut() = text.to_string();
            file.mark_clean();
        }

        Ok(true)
    }

    fn mark_active_dirty(&mut self) {
        if let Some(file) = self.tabs.active_mut() {
            file.mark_dirty();
        }
    }

    fn commit_active(&mut self) {
        let Some(buffer) = self.content.take_content() else {
            return;
        };
        if let Some(file) = self.tabs.active_mut() {
            *file.content_mut() = buffer;
        }
    }

    fn load_active(&mut self) {
        let Some(file) = self.tabs.active() else {
            return;
        };
        self.content.load(file.content().to_string());
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
