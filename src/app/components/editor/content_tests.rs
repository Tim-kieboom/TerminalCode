use super::{Content, StartupArgs};
use crate::app::components::utils::cursor_scroller::Position;
use crate::keybinds::Action;
use std::{fs, path::PathBuf};

fn content() -> Content {
    Content::new(&StartupArgs::new(PathBuf::from(".")))
}

#[test]
fn move_down_to_empty_line_clamps_column_and_scrolls_back() {
    let mut c = content();
    c.context = format!(
        "{}\n{}\n{}\n{}",
        "x".repeat(200),
        "## Notes",
        "",
        "y".repeat(200),
    );

    for _ in 0..200 {
        c.move_curser(Action::ScrollRight);
    }

    let inner_width = 40u16;
    let gutter = c.gutter_width();
    let _ = c.scroller.get_scroll(0, 10, inner_width, gutter);

    c.move_curser(Action::ScrollDown);
    assert_eq!(c.scroller.cursor().horizontal, 8);
    let s1 = c.scroller.get_scroll(1, 10, inner_width, gutter);
    assert_eq!(s1.horizontal, 8 + 7 - 3);

    c.move_curser(Action::ScrollDown);
    assert_eq!(c.scroller.cursor().horizontal, 0);
    let s2 = c.scroller.get_scroll(2, 10, inner_width, gutter);
    assert_eq!(s2.horizontal, 7 - 3);

    let visible = gutter + c.scroller.cursor().horizontal as u16 - s2.horizontal;
    assert!(visible < inner_width);
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
