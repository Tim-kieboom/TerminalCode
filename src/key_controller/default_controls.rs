use crate::{
    key_controller::InsertKind,
    utils::{cursor::Cursor, text_buffer::TextBuffer},
};

/// Moves cursor up one line, preserving column position up to line length.
pub fn move_up(cursor: &mut Cursor, buffer: &TextBuffer) {
    if cursor.line > 0 {
        cursor.line -= 1;
        let target_row = cursor.line as usize;
        cursor.offset = buffer[target_row].len().min(cursor.offset as usize) as u16;
    }
}

/// Moves cursor down one line, preserving column position up to line length.
pub fn move_down(cursor: &mut Cursor, buffer: &TextBuffer) {
    if (cursor.line as usize) + 1 < buffer.line_count() {
        cursor.line += 1;
        let target_row = cursor.line as usize;
        cursor.offset = buffer[target_row].len().min(cursor.offset as usize) as u16;
    }
}

/// Moves cursor left by `amount` columns, wrapping to previous line.
pub fn move_left(cursor: &mut Cursor, buffer: &TextBuffer, amount: u16) {
    if amount <= cursor.offset {
        cursor.offset -= amount;
    } else if cursor.line > 0 {
        cursor.line -= 1;

        let prev = match buffer.get(cursor.line as usize) {
            Some(val) => val,
            None => {
                cursor.line = buffer.line_count() as u16 - 1;
                return;
            }
        };

        let prev_len = prev.len() as u16;
        let leftover = amount - cursor.offset - 1;
        cursor.offset = prev_len.saturating_sub(leftover);
    } else {
        cursor.offset = 0;
    }
}

/// Moves cursor right by `amount` columns, wrapping to next line.
pub fn move_right(cursor: &mut Cursor, buffer: &TextBuffer, amount: u16) {
    let current_line = cursor.line as usize;
    let current = match buffer.get(current_line) {
        Some(val) => val,
        None => {
            cursor.line = buffer.line_count() as u16 - 1;
            return;
        }
    };

    let line_len = current.len() as u16;
    if cursor.offset + amount <= line_len {
        cursor.offset += amount;
    } else if (current_line + 1) < buffer.line_count() {
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

/// Inserts newline at cursor, splitting current line.
pub fn enter(cursor: &mut Cursor, buffer: &mut TextBuffer) {
    let buffer_vec = match buffer.try_as_vec_mut() {
        Some(val) => val,
        None => return,
    };

    let line = cursor.line as usize;
    let offset = cursor.offset as usize;

    let current_line = std::mem::take(&mut buffer_vec[line]);
    let (left, right) = current_line.split_at(offset);

    buffer_vec[line] = left.to_string();
    buffer_vec.insert(line + 1, right.to_string());
    cursor.line += 1;
    cursor.offset = 0;
}

/// Deletes character before cursor (backspace behavior).
pub fn remove(cursor: &mut Cursor, buffer: &mut TextBuffer) {
    match buffer {
        TextBuffer::Single(line) => remove_single_line(cursor, line),
        TextBuffer::Multi(items) => remove_multi_line(cursor, items),
    }
}

/// Inserts text at cursor position.
pub fn insert(cursor: &mut Cursor, buffer: &mut TextBuffer, insert: InsertKind) {
    match buffer {
        TextBuffer::Single(line) => insert_single_line(cursor, line, insert),
        TextBuffer::Multi(items) => insert_multi_line(cursor, items, insert),
    }
}

fn remove_multi_line(cursor: &mut Cursor, buffer: &mut Vec<String>) {
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

fn remove_single_line(cursor: &mut Cursor, buffer: &mut [String; 1]) {
    let column = cursor.offset as usize;

    if column > 0 {
        buffer[0].remove(column - 1);
        cursor.offset -= 1;
    }
}

fn insert_multi_line(cursor: &mut Cursor, buffer: &mut Vec<String>, insert: InsertKind) {
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

fn insert_single_line(cursor: &mut Cursor, buffer: &mut [String; 1], insert: InsertKind) {
    let mut char_buffer = [0u8; 4];
    let text = match &insert {
        InsertKind::Char(char) => &*char.encode_utf8(&mut char_buffer),
        InsertKind::UpperCase(to_uppercase) => &to_uppercase.to_string(),
    };

    let offset = cursor.offset as usize;

    buffer[0].insert_str(offset, text);
    cursor.offset += text.len() as u16;
}
