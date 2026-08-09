use crate::{
    app::components::utils::cursor_scroller::{CursorScroller, HeightScroll, Position, ScrollMode},
    keybinds::Action,
};

impl CursorScroller {
    pub fn move_editor_cursor(
        &mut self,
        action: Action,
        lines_len: usize,
        line_len: impl Fn(usize) -> usize,
        line_chars: impl Fn(usize) -> Vec<char>,
    ) {
        debug_assert!(
            self.mode == ScrollMode::TextEditor,
            "CursorScroller::move_editor_cursor should only be used by mode ScrollMode::TextEditor"
        );

        if lines_len == 0 {
            return;
        }

        match action {
            Action::ScrollWordLeft => self.move_word_left(lines_len, line_len, line_chars),
            Action::ScrollWordRight => self.move_word_right(lines_len, line_chars),
            Action::ScrollLeft => self.move_left(lines_len, line_len),
            Action::ScrollRight => self.move_right(lines_len, line_len),
            Action::ScrollUp
            | Action::ScrollDown
            | Action::ScrollPageUp
            | Action::ScrollPageDown
            | Action::ScrollTop
            | Action::ScrollBottom => {
                self.move_cursor(action, lines_len, 0);
                let len = line_len(self.cursor.vertical);
                self.cursor.horizontal = self.preferred_horizontal.min(len);
            }
            _ => {}
        }
    }

    pub(super) fn scroll_editor(
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

        let col = self.cursor.horizontal as u16 + gutter_width;
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

    fn move_left(&mut self, lines_len: usize, line_len: impl Fn(usize) -> usize) {
        if self.cursor.horizontal == 0 && self.cursor.vertical > 0 {
            self.move_cursor(Action::ScrollUp, lines_len, 0);
            let prev = self.cursor.vertical;
            self.set_cursor(Position {
                vertical: prev,
                horizontal: line_len(prev),
            });
        } else {
            let current = self.cursor.vertical;
            self.move_cursor(Action::ScrollLeft, lines_len, line_len(current));
            self.set_cursor(self.cursor);
        }
    }

    fn move_right(&mut self, lines_len: usize, line_len: impl Fn(usize) -> usize) {
        let current = self.cursor.vertical;
        if self.cursor.horizontal >= line_len(current) && self.cursor.vertical + 1 < lines_len {
            self.move_cursor(Action::ScrollDown, lines_len, 0);
            let next = self.cursor.vertical;
            self.set_cursor(Position {
                vertical: next,
                horizontal: 0,
            });
        } else {
            self.move_cursor(Action::ScrollRight, lines_len, line_len(current));
            self.set_cursor(self.cursor);
        }
    }

    fn move_word_left(
        &mut self,
        lines_len: usize,
        line_len: impl Fn(usize) -> usize,
        line_chars: impl Fn(usize) -> Vec<char>,
    ) {
        let position = self.cursor;
        let chars = line_chars(position.vertical);
        let target = word_left(&chars, position.horizontal);
        if target < position.horizontal {
            self.set_cursor(Position {
                vertical: position.vertical,
                horizontal: target,
            });
        } else if position.vertical > 0 {
            self.move_cursor(Action::ScrollUp, lines_len, 0);
            let prev = self.cursor.vertical;
            self.set_cursor(Position {
                vertical: prev,
                horizontal: line_len(prev),
            });
        }
    }

    fn move_word_right(&mut self, lines_len: usize, line_chars: impl Fn(usize) -> Vec<char>) {
        let position = self.cursor;
        let chars = line_chars(position.vertical);
        let target = word_right(&chars, position.horizontal);
        if target > position.horizontal {
            self.set_cursor(Position {
                vertical: position.vertical,
                horizontal: target,
            });
        } else if position.vertical + 1 < lines_len {
            self.move_cursor(Action::ScrollDown, lines_len, 0);
            let next = self.cursor.vertical;
            self.set_cursor(Position {
                vertical: next,
                horizontal: 0,
            });
        }
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn word_left(line: &[char], start: usize) -> usize {
    let mut i = start;
    while i > 0 && !is_word_char(line[i - 1]) {
        i -= 1;
    }
    while i > 0 && is_word_char(line[i - 1]) {
        i -= 1;
    }
    i
}

fn word_right(line: &[char], start: usize) -> usize {
    let mut i = start;
    while i < line.len() && !is_word_char(line[i]) {
        i += 1;
    }
    while i < line.len() && is_word_char(line[i]) {
        i += 1;
    }
    i
}
