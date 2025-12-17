use crate::{key_controller::InsertKind, window::text_editor::Cursor};

pub fn move_up(cursor: &mut Cursor, buffer: &[String]) {
    if cursor.line > 0 {
        cursor.line -= 1;
        let target_row = cursor.line as usize;
        cursor.offset = buffer[target_row].len().min(cursor.offset as usize) as u16;
    }
}

pub fn move_down(cursor: &mut Cursor, buffer: &[String]) {
    if (cursor.line as usize) + 1 < buffer.len() {
        cursor.line += 1;
        let target_row = cursor.line as usize;
        cursor.offset = buffer[target_row].len().min(cursor.offset as usize) as u16;
    }
}

pub fn move_left(cursor: &mut Cursor, buffer: &[String], amount: u16) {
    if amount <= cursor.offset {
        cursor.offset -= amount;
    } else if cursor.line > 0 {
        cursor.line -= 1;
        let prev_len = buffer[cursor.line as usize].len() as u16;
        let leftover = amount - cursor.offset - 1;
        cursor.offset = prev_len.saturating_sub(leftover);
    } else {
        cursor.offset = 0;
    }
}

pub fn move_right(cursor: &mut Cursor, buffer: &[String], amount: u16) {
    let current_line = cursor.line as usize;
    let line_len = buffer[current_line].len() as u16;

    if cursor.offset + amount <= line_len {
        cursor.offset += amount;
    } else if (current_line + 1) < buffer.len() {
        let mut leftover = amount - (line_len - cursor.offset);
        cursor.line += 1;
        cursor.offset = 0;

        let next_len = buffer[cursor.line as usize].len() as u16;
        if leftover > next_len {
            leftover = next_len;
        }
        cursor.offset = leftover;
    } else {
        cursor.offset = line_len;
    }
}

pub fn enter(cursor: &mut Cursor, buffer: &mut Vec<String>) {
    let line = cursor.line as usize;
    let offset = cursor.offset as usize;

    let current_line = buffer[line].clone();
    let (left, right) = current_line.split_at(offset);

    buffer[line] = left.to_string();
    buffer.insert(line + 1, right.to_string());
    cursor.line += 1;
    cursor.offset = 0;
}

pub fn remove_multi_line(cursor: &mut Cursor, buffer: &mut Vec<String>) {
    let row = cursor.line as usize;
    let column = cursor.offset as usize;

    if column > 0 {
        buffer[row].remove(column - 1);
        cursor.offset -= 1;
    } else if row > 0 {
        let prev_len = buffer[row - 1].len();
        let removed = buffer.remove(row);
        buffer[row - 1].push_str(&removed);
        cursor.line -= 1;
        cursor.offset = prev_len as u16;
    }
}

pub fn remove_single_line(cursor: &mut Cursor, buffer: &mut String) {
    let column = cursor.offset as usize;

    if column > 0 {
        buffer.remove(column - 1);
        cursor.offset -= 1;
    }
}

pub fn insert_multi_line(cursor: &mut Cursor, buffer: &mut Vec<String>, insert: InsertKind) {
    let mut char_buffer = [0u8; 4];
    let text = match &insert {
        InsertKind::Char(char) => &*char.encode_utf8(&mut char_buffer),
        InsertKind::UpperCase(to_uppercase) => &to_uppercase.to_string(),
    };

    let line = cursor.line as usize;
    let offset = cursor.offset as usize;

    buffer[line].insert_str(offset, text);
    cursor.offset += text.len() as u16;
}

pub fn insert_single_line(cursor: &mut Cursor, buffer: &mut String, insert: InsertKind) {
    let mut char_buffer = [0u8; 4];
    let text = match &insert {
        InsertKind::Char(char) => &*char.encode_utf8(&mut char_buffer),
        InsertKind::UpperCase(to_uppercase) => &to_uppercase.to_string(),
    };

    let offset = cursor.offset as usize;

    buffer.insert_str(offset, text);
    cursor.offset += text.len() as u16;
}
