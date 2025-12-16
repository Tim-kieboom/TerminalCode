use crate::{key_controller::InputEvent, window::text_editor::Cursor};

pub enum SessionEvent {
    Exit,
    Back,
    SaveFile,
    OpenLookup,
}

pub struct KeyController<'a> {
    cursor: &'a mut Cursor,
    buffer: &'a mut Vec<String>,
    file_saved: &'a mut bool,
}
impl<'a> KeyController<'a> {
    pub fn new(
        cursor: &'a mut Cursor,
        buffer: &'a mut Vec<String>,
        file_saved: &'a mut bool,
    ) -> Self {
        Self {
            cursor,
            buffer,
            file_saved,
        }
    }

    pub fn handle_input(&mut self, event: InputEvent) -> Option<SessionEvent> {
        //helper to early exit if window is one line
        fn expect_multi_line<'a>(this: &KeyController<'a>) -> Option<()> {
            if this.buffer.len() == 1 {
                None
            } else {
                Some(())
            }
        }

        match event {
            InputEvent::None => (),
            InputEvent::Left => self.move_left(1),
            InputEvent::Right => self.move_right(1),
            InputEvent::Up => {
                expect_multi_line(self)?;
                self.move_up();
            }
            InputEvent::Down => {
                expect_multi_line(self)?;
                self.move_down();
            }
            InputEvent::Exit => return Some(SessionEvent::Exit),
            InputEvent::Back => return Some(SessionEvent::Back),
            InputEvent::Enter => {
                expect_multi_line(self)?;
                self.split_next_line();
            }
            InputEvent::Remove => self.remove_current(),
            InputEvent::Insert(char) => self.insert_char(char),
            InputEvent::SaveFile => return Some(SessionEvent::SaveFile),
            InputEvent::OpenLookup => return Some(SessionEvent::OpenLookup),
        };

        None
    }

    fn move_up(&mut self) {
        if self.cursor.line > 0 {
            self.cursor.line -= 1;
            let target_row = self.cursor.line as usize;
            self.cursor.offset = self.buffer[target_row]
                .len()
                .min(self.cursor.offset as usize) as u16;
        }
    }

    fn move_down(&mut self) {
        if (self.cursor.line as usize) + 1 < self.buffer.len() {
            self.cursor.line += 1;
            let target_row = self.cursor.line as usize;
            self.cursor.offset = self.buffer[target_row]
                .len()
                .min(self.cursor.offset as usize) as u16;
        }
    }

    fn move_left(&mut self, amount: u16) {
        if amount <= self.cursor.offset {
            self.cursor.offset -= amount;
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            let prev_len = self.buffer[self.cursor.line as usize].len() as u16;
            let leftover = amount - self.cursor.offset - 1;
            self.cursor.offset = prev_len.saturating_sub(leftover);
        } else {
            self.cursor.offset = 0;
        }
    }

    fn move_right(&mut self, amount: u16) {
        let current_line = self.cursor.line as usize;
        let line_len = self.buffer[current_line].len() as u16;

        if self.cursor.offset + amount <= line_len {
            self.cursor.offset += amount;
        } else if (current_line + 1) < self.buffer.len() {
            let mut leftover = amount - (line_len - self.cursor.offset);
            self.cursor.line += 1;
            self.cursor.offset = 0;

            let next_len = self.buffer[self.cursor.line as usize].len() as u16;
            if leftover > next_len {
                leftover = next_len;
            }
            self.cursor.offset = leftover;
        } else {
            self.cursor.offset = line_len;
        }
    }

    /// normal `enter`
    fn split_next_line(&mut self) {
        let line = self.cursor.line as usize;
        let offset = self.cursor.offset as usize;

        let current_line = self.buffer[line].clone();
        let (left, right) = current_line.split_at(offset);

        self.buffer[line] = left.to_string();
        self.buffer.insert(line + 1, right.to_string());
        self.cursor.line += 1;
        self.cursor.offset = 0;

        *self.file_saved = false;
    }

    /// normal `backspace`
    fn remove_current(&mut self) {
        let row = self.cursor.line as usize;
        let column = self.cursor.offset as usize;

        if column > 0 {
            self.buffer[row].remove(column - 1);
            self.cursor.offset -= 1;
        } else if row > 0 {
            let prev_len = self.buffer[row - 1].len();
            let removed = self.buffer.remove(row);
            self.buffer[row - 1].push_str(&removed);
            self.cursor.line -= 1;
            self.cursor.offset = prev_len as u16;
        }

        *self.file_saved = false;
    }

    fn insert_char(&mut self, char: char) {
        let row = self.cursor.line as usize;
        let column = self.cursor.offset as usize;

        self.buffer[row].insert(column, char);
        self.cursor.offset += 1;

        *self.file_saved = false;
    }
}
