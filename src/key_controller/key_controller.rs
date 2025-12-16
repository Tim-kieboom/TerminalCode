use crate::{key_controller::{InputEvent, InsertKind}, session::FileContext, window::text_editor::Cursor};

pub enum SessionEvent {
    Exit,
    Back,
    SaveFile,
    OnInsert,
    OnRemove,
    OpenLookup,
}

pub struct KeyController<'a> {
    cursor: &'a mut Cursor,
    buffer: &'a mut Vec<String>,
    file_context: &'a mut FileContext,
}
impl<'a> KeyController<'a> {
    pub fn new(
        cursor: &'a mut Cursor,
        buffer: &'a mut Vec<String>,
        file_context: &'a mut FileContext,
    ) -> Self {
        Self {
            cursor,
            buffer,
            file_context,
        }
    }

    pub fn handle_input(&mut self, event: InputEvent) -> Option<SessionEvent> {

        match event {
            InputEvent::Left => {
                self.move_left(1);
            }
            InputEvent::Right => {
                self.move_right(1);
            }
            InputEvent::Up => {
                self.move_up();
            }
            InputEvent::Down => {
                self.move_down();
            }
            InputEvent::Enter => {
                self.split_next_line();
                return Some(SessionEvent::OnInsert)
            }
            InputEvent::Remove => {
                self.remove_current();
                return Some(SessionEvent::OnRemove)
            }
            InputEvent::Insert(insert) => {
                self.insert(insert);
                return Some(SessionEvent::OnInsert)
            }

            InputEvent::None => return None,
            InputEvent::Exit => return Some(SessionEvent::Exit),
            InputEvent::Back => return Some(SessionEvent::Back),
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

        self.file_context.file_saved = false;
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

        self.file_context.file_saved = false;
    }

    fn insert(&mut self, insert: InsertKind) {

        let mut buffer = [0u8; 4];
        let text = match &insert {
            InsertKind::Char(char) => &*char.encode_utf8(&mut buffer),
            InsertKind::String(string) => string,
            InsertKind::UpperCase(to_uppercase) => &to_uppercase.to_string(),
        };
        
        let row = self.cursor.line as usize;
        let column = self.cursor.offset as usize;

        self.buffer[row].insert_str(column, text);
        self.cursor.offset += text.len() as u16;

        self.file_context.file_saved = false;
    }
}
