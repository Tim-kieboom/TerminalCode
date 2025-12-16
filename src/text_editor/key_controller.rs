use anyhow::Result;
use crossterm::event::{Event, KeyEventKind, KeyModifiers};
use crate::text_editor::TextEditor;

impl TextEditor {

    pub fn handle_event(&mut self, event: Event) -> Result<SessionEvent> {
        use crossterm::event::KeyCode::*;

        if let Event::Key(key) = event {

            if key.kind != KeyEventKind::Press {
                return Ok(SessionEvent::None)
            }

            match key.code {
                Up => self.move_up(),
                Down => self.move_down(),
                Left => self.move_left(1),
                Right => self.move_right(1),
                Enter => self.split_next_line(),
                Backspace => self.remove_current(),
                Char(c) if key.modifiers.is_empty() => self.insert_char(c),
                
                Esc => return Ok(SessionEvent::Exit),
                Char(c) => return self.key_combine(c, key.modifiers), 
                _ => {}
            }
        }

        Ok(SessionEvent::None)
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

    pub fn move_right(&mut self, amount: u16) {
        let current_line = self.cursor.line as usize;
        let line_len = self.buffer[current_line].len() as u16;

        if self.cursor.offset + amount <= line_len {
            self.cursor.offset += amount;
        } 

        else if (current_line + 1) < self.buffer.len() {
            let mut leftover = amount - (line_len - self.cursor.offset);
            self.cursor.line += 1;
            self.cursor.offset = 0;

            let next_len = self.buffer[self.cursor.line as usize].len() as u16;
            if leftover > next_len {
                leftover = next_len;
            }
            self.cursor.offset = leftover;
        } 
        else {
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

        self.file_saved = false;
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

        self.file_saved = false;
    }

    fn insert_char(&mut self, char: char) {
        let row = self.cursor.line as usize;
        let column = self.cursor.offset as usize;

        self.buffer[row].insert(column, char);
        self.cursor.offset += 1;

        self.file_saved = false;
    }

    fn key_combine(&mut self, char: char, modifier: KeyModifiers) -> Result<SessionEvent> {

        if char == 'p' && modifier == KeyModifiers::CONTROL {
            return Ok(SessionEvent::OpenLookup)
        }
        if char == 's' && modifier == KeyModifiers::CONTROL {
            self.save_file()?;
        }

        Ok(SessionEvent::None)
    }

}

pub enum SessionEvent {
    None,
    Exit,
    OpenLookup,
}