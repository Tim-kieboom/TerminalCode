use super::{Content, StartupArgs};
use crate::app::components::utils::cursor_scroller::Position;
use crate::keybinds::Action;
use std::{fs, path::PathBuf};

fn content() -> Content {
    Content::new(&StartupArgs::new(PathBuf::from(".")))
}

#[test]
fn insert_char_appends_to_line() {
    let mut c = content();
    c.insert_char('h');
    c.insert_char('i');
    assert_eq!(c.context, "hi");
}

#[test]
fn insert_char_handles_multibyte() {
    let mut c = content();
    c.insert_char('é');
    c.insert_char('x');
    assert_eq!(c.context, "éx");
}

#[test]
fn insert_newline_splits_line() {
    let mut c = content();
    for ch in "abc".chars() {
        c.insert_char(ch);
    }
    c.insert_newline();
    assert_eq!(c.context, "abc\n");
    assert_eq!(c.scroller.cursor().vertical, 1);
    assert_eq!(c.scroller.cursor().horizontal, 0);
}

#[test]
fn insert_char_after_newline_edits_next_line() {
    let mut c = content();
    for ch in "abc".chars() {
        c.insert_char(ch);
    }
    c.insert_newline();
    c.insert_char('x');
    assert_eq!(c.context, "abc\nx");
}

#[test]
fn insert_tab_inserts_four_spaces() {
    let mut c = content();
    c.insert_tab();
    assert_eq!(c.context, "    ");
}

#[test]
fn delete_char_at_end_is_noop() {
    let mut c = content();
    for ch in "abc".chars() {
        c.insert_char(ch);
    }
    c.delete_char();
    assert_eq!(c.context, "abc");
}

#[test]
fn delete_char_removes_at_cursor() {
    let mut c = content();
    for ch in "abc".chars() {
        c.insert_char(ch);
    }
    c.move_curser(Action::ScrollLeft);
    c.delete_char();
    assert_eq!(c.context, "ab");
}

#[test]
fn backspace_removes_char_before_cursor() {
    let mut c = content();
    for ch in "abc".chars() {
        c.insert_char(ch);
    }
    c.move_curser(Action::ScrollLeft);
    c.backspace();
    assert_eq!(c.context, "ac");
}

#[test]
fn backspace_at_line_start_joins_lines() {
    let mut c = content();
    for ch in "ab".chars() {
        c.insert_char(ch);
    }
    c.insert_newline();
    c.insert_char('c');
    c.insert_char('d');
    c.move_curser(Action::ScrollLeft);
    c.move_curser(Action::ScrollLeft);
    c.backspace();
    assert_eq!(c.context, "abcd");
}

#[test]
fn open_reads_file_and_normalizes_newlines() {
    let dir = std::env::temp_dir().join(format!("content_open_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("file.txt");
    fs::write(&path, "one\r\ntwo").unwrap();

    let mut c = content();
    c.open(&path).unwrap();
    assert_eq!(c.context, "one\ntwo");
    assert_eq!(c.scroller.cursor(), Position::default());

    let _ = fs::remove_dir_all(&dir);
}
