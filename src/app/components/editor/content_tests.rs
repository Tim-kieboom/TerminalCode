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
    assert_eq!(c.scroller.position().horizontal, 8);
    let s1 = c.scroller.get_scroll(1, 10, inner_width, gutter);
    assert_eq!(s1.horizontal, 8 + 7 - 3);

    c.move_curser(Action::ScrollDown);
    assert_eq!(c.scroller.position().horizontal, 0);
    let s2 = c.scroller.get_scroll(2, 10, inner_width, gutter);
    assert_eq!(s2.horizontal, 7 - 3);

    let visible = gutter + c.scroller.position().horizontal as u16 - s2.horizontal;
    assert!(visible < inner_width);
}

#[test]
fn left_at_line_start_moves_to_prev_line_end() {
    let mut c = content();
    c.context = "abcd\ne".to_string();
    c.scroller.set_position(Position {
        vertical: 1,
        horizontal: 0,
    });
    c.move_curser(Action::ScrollLeft);
    assert_eq!(
        c.scroller.position(),
        Position {
            vertical: 0,
            horizontal: 4,
        }
    );
}

#[test]
fn right_at_line_end_moves_to_next_line_start() {
    let mut c = content();
    c.context = "abcd\ne".to_string();
    c.scroller.set_position(Position {
        vertical: 0,
        horizontal: 4,
    });
    c.move_curser(Action::ScrollRight);
    assert_eq!(
        c.scroller.position(),
        Position {
            vertical: 1,
            horizontal: 0,
        }
    );
}

#[test]
fn left_on_empty_line_moves_to_prev_line_end() {
    let mut c = content();
    c.context = "ab\n".to_string();
    c.scroller.set_position(Position {
        vertical: 1,
        horizontal: 0,
    });
    c.move_curser(Action::ScrollLeft);
    assert_eq!(
        c.scroller.position(),
        Position {
            vertical: 0,
            horizontal: 2,
        }
    );
}

#[test]
fn right_on_empty_line_moves_to_next_line_start() {
    let mut c = content();
    c.context = "\nab".to_string();
    c.scroller.set_position(Position {
        vertical: 0,
        horizontal: 0,
    });
    c.move_curser(Action::ScrollRight);
    assert_eq!(
        c.scroller.position(),
        Position {
            vertical: 1,
            horizontal: 0,
        }
    );
}

#[test]
fn left_at_first_line_start_stays() {
    let mut c = content();
    c.context = "ab\ncd".to_string();
    c.move_curser(Action::ScrollLeft);
    assert_eq!(
        c.scroller.position(),
        Position {
            vertical: 0,
            horizontal: 0,
        }
    );
}

#[test]
fn right_at_last_line_end_stays() {
    let mut c = content();
    c.context = "ab\ncd".to_string();
    c.scroller.set_position(Position {
        vertical: 1,
        horizontal: 2,
    });
    c.move_curser(Action::ScrollRight);
    assert_eq!(
        c.scroller.position(),
        Position {
            vertical: 1,
            horizontal: 2,
        }
    );
}

#[test]
fn word_right_moves_past_current_word() {
    let mut c = content();
    c.context = "hello world".to_string();
    c.scroller.set_position(Position {
        vertical: 0,
        horizontal: 0,
    });
    c.move_curser(Action::ScrollWordRight);
    assert_eq!(c.scroller.horizontal(), 5);
    c.move_curser(Action::ScrollWordRight);
    assert_eq!(c.scroller.horizontal(), 11);
}

#[test]
fn word_left_moves_to_word_start() {
    let mut c = content();
    c.context = "hello world".to_string();
    c.scroller.set_position(Position {
        vertical: 0,
        horizontal: 11,
    });
    c.move_curser(Action::ScrollWordLeft);
    assert_eq!(c.scroller.horizontal(), 6);
    c.move_curser(Action::ScrollWordLeft);
    assert_eq!(c.scroller.horizontal(), 0);
}

#[test]
fn word_right_skips_multiple_spaces() {
    let mut c = content();
    c.context = "a   b".to_string();
    c.scroller.set_position(Position {
        vertical: 0,
        horizontal: 0,
    });
    c.move_curser(Action::ScrollWordRight);
    assert_eq!(c.scroller.horizontal(), 1);
    c.move_curser(Action::ScrollWordRight);
    assert_eq!(c.scroller.horizontal(), 5);
}

#[test]
fn word_right_wraps_to_next_line_start() {
    let mut c = content();
    c.context = "hello\nworld".to_string();
    c.scroller.set_position(Position {
        vertical: 0,
        horizontal: 5,
    });
    c.move_curser(Action::ScrollWordRight);
    assert_eq!(
        c.scroller.position(),
        Position {
            vertical: 1,
            horizontal: 0,
        }
    );
}

#[test]
fn word_left_wraps_to_prev_line_end() {
    let mut c = content();
    c.context = "hello\nworld".to_string();
    c.scroller.set_position(Position {
        vertical: 1,
        horizontal: 0,
    });
    c.move_curser(Action::ScrollWordLeft);
    assert_eq!(
        c.scroller.position(),
        Position {
            vertical: 0,
            horizontal: 5,
        }
    );
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
    assert_eq!(c.scroller.position().vertical, 1);
    assert_eq!(c.scroller.position().horizontal, 0);
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
fn vertical_move_restores_preferred_column_on_long_lines() {
    let mut c = content();
    c.context = format!("{}\nshort\n\n{}", "x".repeat(200), "y".repeat(200));

    for _ in 0..200 {
        c.move_curser(Action::ScrollRight);
    }
    assert_eq!(c.scroller.position().horizontal, 200);

    c.move_curser(Action::ScrollDown);
    assert_eq!(c.scroller.position().horizontal, 5);

    c.move_curser(Action::ScrollDown);
    assert_eq!(c.scroller.position().horizontal, 0);

    c.move_curser(Action::ScrollDown);
    assert_eq!(c.scroller.position().horizontal, 200);
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
    assert_eq!(c.scroller.position(), Position::default());

    let _ = fs::remove_dir_all(&dir);
}
