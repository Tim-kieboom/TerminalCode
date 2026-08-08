mod editor_mode;
mod list_mode;

#[cfg(test)]
#[path = "cursor_scroller_tests.rs"]
mod tests;

use crate::keybinds::Action;

#[derive(PartialEq, Eq)]
pub enum ScrollMode {
    List,
    TextEditor,
}

pub struct CursorScroller {
    mode: ScrollMode,

    vertical_offset: u16,
    horizontal_offset: u16,
    cursor: Position<usize>,
    direction: ScrollDirection,
    preferred_horizontal: usize,
}

impl CursorScroller {
    pub fn new(mode: ScrollMode) -> Self {
        Self {
            mode,
            vertical_offset: 0,
            horizontal_offset: 0,
            cursor: Position::default(),
            direction: ScrollDirection {
                height: HeightScroll::Up,
            },
            preferred_horizontal: 0,
        }
    }

    pub fn cursor(&self) -> Position<usize> {
        self.cursor
    }

    pub fn vertical(&self) -> usize {
        self.cursor.vertical
    }

    pub fn horizontal(&self) -> usize {
        self.cursor.horizontal
    }

    pub fn set_cursor(&mut self, position: Position<usize>) {
        self.preferred_horizontal = position.horizontal;
        self.cursor = position;
    }

    pub fn move_cursor(&mut self, action: Action, length: usize, width: usize) {
        if length == 0 {
            return;
        }

        match action {
            Action::ScrollUp => {
                self.direction.height = HeightScroll::Up;
                self.cursor.vertical = self.cursor.vertical.saturating_sub(1);
            }
            Action::ScrollDown => {
                self.direction.height = HeightScroll::Down;
                self.cursor.vertical = self.cursor.vertical.saturating_add(1).min(length - 1);
            }
            Action::ScrollLeft => {
                self.cursor.horizontal = self.cursor.horizontal.saturating_sub(1);
            }
            Action::ScrollRight => {
                self.cursor.horizontal = self.cursor.horizontal.saturating_add(1).min(width);
            }
            Action::ScrollPageUp => {
                self.direction.height = HeightScroll::Up;
                self.cursor.vertical = self.cursor.vertical.saturating_sub(10);
            }
            Action::ScrollPageDown => {
                self.direction.height = HeightScroll::Down;
                self.cursor.vertical = self.cursor.vertical.saturating_add(10).min(length - 1);
            }
            Action::ScrollTop => {
                self.direction.height = HeightScroll::Up;
                self.cursor.vertical = 0;
            }
            Action::ScrollBottom => {
                self.direction.height = HeightScroll::Down;
                self.cursor.vertical = length - 1;
            }
            _ => {}
        }
    }

    pub fn get_scroll(
        &mut self,
        cursor_visual_line: u16,
        height: u16,
        width: u16,
        gutter_width: u16,
    ) -> Position<u16> {
        if height == 0 {
            return Position::default();
        }

        match self.mode {
            ScrollMode::List => self.scroll_list(cursor_visual_line, height),
            ScrollMode::TextEditor => {
                self.scroll_editor(cursor_visual_line, height, width, gutter_width)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position<T> {
    pub vertical: T,
    pub horizontal: T,
}

#[derive(Debug, Clone, Copy)]
struct ScrollDirection {
    height: HeightScroll,
}

#[derive(Debug, Clone, Copy)]
enum HeightScroll {
    Up,
    Down,
}
