use super::{Content, StartupArgs};
use crate::app::components::utils::cursor_scroller::Position;
use crate::keybinds::Action;
use ropey::Rope;
use std::path::PathBuf;

fn content() -> Content {
    Content::new(&StartupArgs::new(PathBuf::from(".")))
}

fn content_set_string(this: &mut Content, str: impl Into<Rope>) {
    this.content = Some(str.into());
}

fn content_str_eq(this: &Content, str: &str) -> bool {
    matches!(&this.content, Some(content) if content == str)
}

#[test]
fn move_cursor_delegates_to_scroller() {
    let mut c = content();
    content_set_string(&mut c, "abcd\ne");
    c.scroller.set_cursor(Position {
        vertical: 1,
        horizontal: 0,
    });
    c.move_cursor(Action::ScrollLeft);
    assert_eq!(
        c.scroller.cursor(),
        Position {
            vertical: 0,
            horizontal: 4,
        }
    );
}

#[test]
fn insert_char_appends_to_line() {
    let mut c = content();
    content_set_string(&mut c, "");
    c.insert_char('h');
    c.insert_char('i');
    assert!(content_str_eq(&c, "hi"));
}

#[test]
fn insert_char_handles_multibyte() {
    let mut c = content();
    content_set_string(&mut c, "");
    c.insert_char('é');
    c.insert_char('x');
    assert!(content_str_eq(&c, "éx"));
}

#[test]
fn insert_newline_splits_line() {
    let mut c = content();
    content_set_string(&mut c, "");
    for ch in "abc".chars() {
        c.insert_char(ch);
    }
    c.insert_newline();
    assert!(content_str_eq(&c, "abc\n"));
    assert_eq!(c.scroller.cursor().vertical, 1);
    assert_eq!(c.scroller.cursor().horizontal, 0);
}

#[test]
fn insert_char_after_newline_edits_next_line() {
    let mut c = content();
    content_set_string(&mut c, "");
    for ch in "abc".chars() {
        c.insert_char(ch);
    }
    c.insert_newline();
    c.insert_char('x');
    assert!(content_str_eq(&c, "abc\nx"));
}

#[test]
fn insert_tab_inserts_four_spaces() {
    let mut c = content();
    content_set_string(&mut c, "");
    c.insert_tab();
    assert!(content_str_eq(&c, "    "));
}

#[test]
fn delete_char_at_end_is_noop() {
    let mut c = content();
    content_set_string(&mut c, "");
    for ch in "abc".chars() {
        c.insert_char(ch);
    }
    c.delete_char();
    assert!(content_str_eq(&c, "abc"));
}

#[test]
fn delete_char_removes_at_cursor() {
    let mut c = content();
    content_set_string(&mut c, "");
    for ch in "abc".chars() {
        c.insert_char(ch);
    }
    c.move_cursor(Action::ScrollLeft);
    c.delete_char();
    assert!(content_str_eq(&c, "ab"));
}

#[test]
fn backspace_removes_char_before_cursor() {
    let mut c = content();
    content_set_string(&mut c, "");
    for ch in "abc".chars() {
        c.insert_char(ch);
    }
    c.move_cursor(Action::ScrollLeft);
    c.backspace();
    assert!(content_str_eq(&c, "ac"));
}

#[test]
fn backspace_at_line_start_joins_lines() {
    let mut c = content();
    content_set_string(&mut c, "");
    for ch in "ab".chars() {
        c.insert_char(ch);
    }
    c.insert_newline();
    c.insert_char('c');
    c.insert_char('d');
    c.move_cursor(Action::ScrollLeft);
    c.move_cursor(Action::ScrollLeft);
    c.backspace();
    assert!(content_str_eq(&c, "abcd"));
}

#[test]
fn load_resets_cursor() {
    let mut c = content();
    content_set_string(&mut c, "hello\nworld");
    c.scroller.set_cursor(Position {
        vertical: 1,
        horizontal: 4,
    });

    c.load("x".to_string());
    assert_eq!(c.scroller.cursor(), Position::default());
}

#[test]
fn edits_without_content_are_noops() {
    let mut c = content();
    c.insert_char('a');
    c.delete_char();
    c.insert_newline();
    c.backspace();
    c.move_cursor(Action::ScrollDown);
    assert!(c.content.is_none());
}
