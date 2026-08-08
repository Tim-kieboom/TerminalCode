use crate::keybinds::Action;

#[cfg(test)]
#[path = "cursor_scroller_tests.rs"]
mod tests;

pub enum ScrollMode {
    List,
    TextEditor,
}

pub struct CursorScroller {
    mode: ScrollMode,

    vertical_offset: u16,
    horizontal_offset: u16,
    position: Position<usize>,
    direction: ScrollDirection,
}

impl CursorScroller {
    pub fn new(mode: ScrollMode) -> Self {
        Self {
            mode,
            vertical_offset: 0,
            horizontal_offset: 0,
            position: Position::default(),
            direction: ScrollDirection {
                height: HeightScroll::Up,
            },
        }
    }

    pub fn move_cursor(&mut self, action: Action, length: usize, width: usize) {
        if length == 0 {
            return;
        }

        match action {
            Action::ScrollUp => {
                self.direction.height = HeightScroll::Up;
                self.position.vertical = self.position.vertical.saturating_sub(1);
            }
            Action::ScrollDown => {
                self.direction.height = HeightScroll::Down;
                self.position.vertical = self.position.vertical.saturating_add(1).min(length - 1);
            }
            Action::ScrollLeft => {
                self.position.horizontal = self.position.horizontal.saturating_sub(1);
            }
            Action::ScrollRight => {
                self.position.horizontal = self.position.horizontal.saturating_add(1).min(width);
            }
            Action::ScrollPageUp => {
                self.direction.height = HeightScroll::Up;
                self.position.vertical = self.position.vertical.saturating_sub(10);
            }
            Action::ScrollPageDown => {
                self.direction.height = HeightScroll::Down;
                self.position.vertical = self.position.vertical.saturating_add(10).min(length - 1);
            }
            Action::ScrollTop => {
                self.direction.height = HeightScroll::Up;
                self.position.vertical = 0;
            }
            Action::ScrollBottom => {
                self.direction.height = HeightScroll::Down;
                self.position.vertical = length - 1;
            }
            _ => {}
        }
    }

    pub fn position(&self) -> Position<usize> {
        self.position
    }

    pub fn vertical(&self) -> usize {
        self.position.vertical
    }

    pub fn horizontal(&self) -> usize {
        self.position.horizontal
    }

    pub fn set_position(&mut self, pos: Position<usize>) {
        self.position = pos;
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

    fn scroll_editor(
        &mut self,
        cursor_visual_line: u16,
        height: u16,
        width: u16,
        gutter_width: u16,
    ) -> Position<u16> {
        let margin = 3;
        match self.direction.height {
            HeightScroll::Up => {
                if cursor_visual_line < self.vertical_offset + margin {
                    self.vertical_offset = cursor_visual_line.saturating_sub(margin);
                }
            }
            HeightScroll::Down => {
                if cursor_visual_line + margin >= self.vertical_offset + height {
                    self.vertical_offset = cursor_visual_line + 1 + margin - height;
                }
            }
        }

        let col = self.position.horizontal as u16 + gutter_width;
        if col < self.horizontal_offset + margin {
            self.horizontal_offset = col.saturating_sub(margin);
        }
        if col + margin >= self.horizontal_offset + width {
            self.horizontal_offset = col + 1 + margin - width;
        }

        Position {
            horizontal: self.horizontal_offset,
            vertical: self.vertical_offset,
        }
    }

    fn scroll_list(&mut self, cursor_visual_line: u16, height: u16) -> Position<u16> {
        let margin = 3;
        match self.direction.height {
            HeightScroll::Up => {
                if cursor_visual_line < self.vertical_offset + margin {
                    self.vertical_offset = cursor_visual_line.saturating_sub(margin);
                }
            }
            HeightScroll::Down => {
                if cursor_visual_line + margin >= self.vertical_offset + height {
                    self.vertical_offset = cursor_visual_line + 1 + margin - height;
                }
            }
        }

        Position {
            horizontal: 0,
            vertical: self.vertical_offset,
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
