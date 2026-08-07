use crate::keybinds::Action;

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
}

impl CursorScroller {
    pub fn new(mode: ScrollMode) -> Self {
        Self {
            mode,
            vertical_offset: 0,
            horizontal_offset: 0,
            cursor: Position::default(),
            direction: ScrollDirection {
                width: WidthScroll::Left,
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
                self.cursor.vertical = self.cursor.vertical.saturating_sub(1);
            }
            Action::ScrollDown => {
                self.direction.height = HeightScroll::Down;
                self.cursor.vertical = self.cursor.vertical.saturating_add(1).min(length - 1);
            }
            Action::ScrollLeft => {
                self.direction.width = WidthScroll::Left;
                self.cursor.horizontal = self.cursor.horizontal.saturating_sub(1);
            }
            Action::ScrollRight => {
                self.direction.width = WidthScroll::Right;
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

    pub fn cursor(&self) -> Position<usize> {
        self.cursor
    }

    pub fn set_cursor(&mut self, pos: Position<usize>) {
        self.cursor = pos;
    }

    pub fn clamp_column(&mut self, line_len: usize) {
        self.cursor.horizontal = self.cursor.horizontal.min(line_len);
    }

    pub fn get_scroll(
        &mut self,
        cursor_visual_line: u16,
        height: u16,
        width: u16,
    ) -> Position<u16> {
        if height == 0 {
            return Position::default();
        }

        match self.mode {
            ScrollMode::List => self.scroll_list(cursor_visual_line, height),
            ScrollMode::TextEditor => self.scroll_editor(cursor_visual_line, height, width),
        }
    }

    fn scroll_editor(&mut self, cursor_visual_line: u16, height: u16, width: u16) -> Position<u16> {
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

        let col = self.cursor.horizontal as u16;
        match self.direction.width {
            WidthScroll::Left => {
                if col < self.horizontal_offset + margin {
                    self.horizontal_offset = col.saturating_sub(margin);
                }
            }
            WidthScroll::Right => {
                if col + margin >= self.horizontal_offset + width {
                    self.horizontal_offset = col + 1 + margin - width;
                }
            }
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
    width: WidthScroll,
    height: HeightScroll,
}

#[derive(Debug, Clone, Copy)]
enum HeightScroll {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy)]
enum WidthScroll {
    Left,
    Right,
}

#[cfg(test)]
mod tests;
