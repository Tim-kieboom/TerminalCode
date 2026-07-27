use crate::keybinds::Action;

#[derive(Debug, Clone, Copy)]
enum ScrollDir {
    Up,
    Down,
}

pub enum ScrollMode {
    List,
}

pub struct CursorScroller {
    cursor: usize,
    mode: ScrollMode,

    scroll_offset: u16,
    scroll_direction: ScrollDir,
}

pub type Vertical = u16;
pub type Horizontal = u16;
impl CursorScroller {
    pub fn new(mode: ScrollMode) -> Self {
        Self {
            mode,
            cursor: 0,
            scroll_offset: 0,
            scroll_direction: ScrollDir::Down,
        }
    }

    pub fn move_cursor(&mut self, action: Action, length: usize) {
        if length == 0 {
            return;
        }

        match action {
            Action::ScrollUp => {
                self.scroll_direction = ScrollDir::Up;
                self.cursor = self.cursor.saturating_sub(1);
            }
            Action::ScrollDown => {
                self.scroll_direction = ScrollDir::Down;
                self.cursor = self.cursor.saturating_add(1).min(length - 1);
            }
            Action::ScrollPageUp => {
                self.scroll_direction = ScrollDir::Up;
                self.cursor = self.cursor.saturating_sub(10);
            }
            Action::ScrollPageDown => {
                self.scroll_direction = ScrollDir::Down;
                self.cursor = self.cursor.saturating_add(10).min(length - 1);
            }
            Action::ScrollTop => {
                self.scroll_direction = ScrollDir::Up;
                self.cursor = 0;
            }
            Action::ScrollBottom => {
                self.scroll_direction = ScrollDir::Down;
                self.cursor = length - 1;
            }
            _ => {}
        }
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn get_scroll(&mut self, length: u16, height: u16) -> (Vertical, Horizontal) {
        if height == 0 {
            return (0, 0);
        }

        match self.mode {
            ScrollMode::List => self.scroll_list(length, height),
        }
    }

    fn scroll_list(&mut self, length: u16, height: u16) -> (Vertical, Horizontal) {
        let cursor_visual = self.cursor_line(length);

        match self.scroll_direction {
            ScrollDir::Up => {
                if cursor_visual < self.scroll_offset {
                    self.scroll_offset = cursor_visual;
                }
            }
            ScrollDir::Down => {
                if cursor_visual >= self.scroll_offset + height {
                    self.scroll_offset = cursor_visual - height + 1;
                }
            }
        }

        (self.scroll_offset, 0)
    }

    fn cursor_line(&self, lines_count: u16) -> u16 {
        if (self.cursor as u16) < lines_count {
            1 + self.cursor as u16
        } else {
            3 + self.cursor as u16
        }
    }
}
