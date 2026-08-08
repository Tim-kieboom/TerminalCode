use super::{CursorScroller, Position, ScrollMode};
use crate::keybinds::Action;

#[test]
fn new_starts_at_origin() {
    let scroller = CursorScroller::new(ScrollMode::List);
    assert_eq!(scroller.cursor(), Position::default());
}

#[test]
fn move_cursor_is_noop_for_empty_list() {
    let mut scroller = CursorScroller::new(ScrollMode::List);
    scroller.move_cursor(Action::ScrollDown, 0, 0);
    scroller.move_cursor(Action::ScrollBottom, 0, 0);
    assert_eq!(scroller.cursor(), Position::default());
}

#[test]
fn scroll_down_clamps_at_last_item() {
    let mut scroller = CursorScroller::new(ScrollMode::List);
    for _ in 0..10 {
        scroller.move_cursor(Action::ScrollDown, 5, 0);
    }
    assert_eq!(scroller.cursor().vertical, 4);
}

#[test]
fn scroll_up_saturates_at_zero() {
    let mut scroller = CursorScroller::new(ScrollMode::List);
    scroller.move_cursor(Action::ScrollDown, 5, 0);
    scroller.move_cursor(Action::ScrollUp, 5, 0);
    scroller.move_cursor(Action::ScrollUp, 5, 0);
    assert_eq!(scroller.cursor().vertical, 0);
}

#[test]
fn scroll_top_and_bottom() {
    let mut scroller = CursorScroller::new(ScrollMode::List);
    scroller.move_cursor(Action::ScrollDown, 5, 0);
    scroller.move_cursor(Action::ScrollBottom, 5, 0);
    assert_eq!(scroller.cursor().vertical, 4);

    scroller.move_cursor(Action::ScrollTop, 5, 0);
    assert_eq!(scroller.cursor().vertical, 0);
}

#[test]
fn scroll_right_clamps_at_width() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);
    for _ in 0..10 {
        scroller.move_cursor(Action::ScrollRight, 5, 3);
    }
    assert_eq!(scroller.cursor().horizontal, 3);
}

#[test]
fn scroll_page_down_clamps_at_last() {
    let mut scroller = CursorScroller::new(ScrollMode::List);
    scroller.move_cursor(Action::ScrollPageDown, 100, 0);
    assert_eq!(scroller.cursor().vertical, 10);
    scroller.move_cursor(Action::ScrollPageDown, 100, 0);
    scroller.move_cursor(Action::ScrollPageDown, 100, 0);
    assert_eq!(scroller.cursor().vertical, 30);
    scroller.move_cursor(Action::ScrollPageDown, 25, 0);
    assert_eq!(scroller.cursor().vertical, 24);
}

#[test]
fn get_scroll_returns_default_for_zero_height() {
    let mut scroller = CursorScroller::new(ScrollMode::List);
    assert_eq!(scroller.get_scroll(5, 0, 0, 0), Position::default());
}

#[test]
fn list_scroll_down_offsets_after_margin() {
    let mut scroller = CursorScroller::new(ScrollMode::List);
    for _ in 0..12 {
        scroller.move_cursor(Action::ScrollDown, 30, 0);
    }
    let scroll = scroller.get_scroll(12, 10, 0, 0);
    assert_eq!(scroll.vertical, 12 + 1 + 3 - 10);
}

#[test]
fn list_scroll_up_reduces_offset() {
    let mut scroller = CursorScroller::new(ScrollMode::List);
    for _ in 0..20 {
        scroller.move_cursor(Action::ScrollDown, 30, 0);
    }
    let _ = scroller.get_scroll(20, 10, 0, 0);
    for _ in 0..5 {
        scroller.move_cursor(Action::ScrollUp, 30, 0);
    }
    let scroll = scroller.get_scroll(15, 10, 0, 0);
    assert_eq!(scroll.vertical, 15 - 3);
}

#[test]
fn list_scroll_stays_zero_while_inside_height() {
    let mut scroller = CursorScroller::new(ScrollMode::List);
    scroller.move_cursor(Action::ScrollDown, 30, 0);
    let scroll = scroller.get_scroll(1, 10, 0, 0);
    assert_eq!(scroll.vertical, 0);
}

#[test]
fn editor_scrolls_horizontally() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);
    for _ in 0..20 {
        scroller.move_cursor(Action::ScrollRight, 30, 20);
    }
    let scroll = scroller.get_scroll(0, 10, 20, 0);
    assert_eq!(scroll.horizontal, 20 + 1 + 3 - 20);
}

#[test]
fn editor_does_not_scroll_horizontally_inside_width() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);
    scroller.move_cursor(Action::ScrollRight, 30, 20);
    let scroll = scroller.get_scroll(0, 10, 20, 0);
    assert_eq!(scroll.horizontal, 0);
}

#[test]
fn editor_scrolls_horizontally_with_gutter() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);
    for _ in 0..20 {
        scroller.move_cursor(Action::ScrollRight, 30, 20);
    }
    let scroll = scroller.get_scroll(0, 10, 20, 7);
    assert_eq!(scroll.horizontal, 7 + 20 + 1 + 3 - 20);
}

#[test]
fn editor_scrolls_back_left_when_moving_up_to_short_line() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);

    for _ in 0..30 {
        scroller.move_cursor(Action::ScrollRight, 2, 30);
    }
    let _ = scroller.get_scroll(0, 10, 20, 7);
    assert_eq!(
        scroller.get_scroll(0, 10, 20, 7).horizontal,
        30 + 7 + 1 + 3 - 20
    );

    scroller.move_cursor(Action::ScrollDown, 2, 1);
    scroller.set_cursor(Position {
        vertical: 1,
        horizontal: 1,
    });
    let scroll = scroller.get_scroll(1, 10, 20, 7);
    assert_eq!(scroll.horizontal, 1 + 7 - 3);
}

fn move_editor(scroller: &mut CursorScroller, action: Action, lines: &[&str]) {
    let line_len = |v: usize| lines[v].chars().count();
    let line_chars = |v: usize| lines[v].chars().collect::<Vec<char>>();
    scroller.move_editor_cursor(action, lines.len(), line_len, line_chars);
}

#[test]
fn left_at_line_start_moves_to_prev_line_end() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);
    let lines = ["abcd", "e"];
    scroller.set_cursor(Position {
        vertical: 1,
        horizontal: 0,
    });
    move_editor(&mut scroller, Action::ScrollLeft, &lines);
    assert_eq!(
        scroller.cursor(),
        Position {
            vertical: 0,
            horizontal: 4,
        }
    );
}

#[test]
fn right_at_line_end_moves_to_next_line_start() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);
    let lines = ["abcd", "e"];
    scroller.set_cursor(Position {
        vertical: 0,
        horizontal: 4,
    });
    move_editor(&mut scroller, Action::ScrollRight, &lines);
    assert_eq!(
        scroller.cursor(),
        Position {
            vertical: 1,
            horizontal: 0,
        }
    );
}

#[test]
fn left_on_empty_line_moves_to_prev_line_end() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);
    let lines = ["ab", ""];
    scroller.set_cursor(Position {
        vertical: 1,
        horizontal: 0,
    });
    move_editor(&mut scroller, Action::ScrollLeft, &lines);
    assert_eq!(
        scroller.cursor(),
        Position {
            vertical: 0,
            horizontal: 2,
        }
    );
}

#[test]
fn right_on_empty_line_moves_to_next_line_start() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);
    let lines = ["", "ab"];
    scroller.set_cursor(Position {
        vertical: 0,
        horizontal: 0,
    });
    move_editor(&mut scroller, Action::ScrollRight, &lines);
    assert_eq!(
        scroller.cursor(),
        Position {
            vertical: 1,
            horizontal: 0,
        }
    );
}

#[test]
fn left_at_first_line_start_stays() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);
    let lines = ["ab", "cd"];
    move_editor(&mut scroller, Action::ScrollLeft, &lines);
    assert_eq!(
        scroller.cursor(),
        Position {
            vertical: 0,
            horizontal: 0,
        }
    );
}

#[test]
fn right_at_last_line_end_stays() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);
    let lines = ["ab", "cd"];
    scroller.set_cursor(Position {
        vertical: 1,
        horizontal: 2,
    });
    move_editor(&mut scroller, Action::ScrollRight, &lines);
    assert_eq!(
        scroller.cursor(),
        Position {
            vertical: 1,
            horizontal: 2,
        }
    );
}

#[test]
fn word_right_moves_past_current_word() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);
    let lines = ["hello world"];
    move_editor(&mut scroller, Action::ScrollWordRight, &lines);
    assert_eq!(scroller.horizontal(), 5);
    move_editor(&mut scroller, Action::ScrollWordRight, &lines);
    assert_eq!(scroller.horizontal(), 11);
}

#[test]
fn word_left_moves_to_word_start() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);
    let lines = ["hello world"];
    scroller.set_cursor(Position {
        vertical: 0,
        horizontal: 11,
    });
    move_editor(&mut scroller, Action::ScrollWordLeft, &lines);
    assert_eq!(scroller.horizontal(), 6);
    move_editor(&mut scroller, Action::ScrollWordLeft, &lines);
    assert_eq!(scroller.horizontal(), 0);
}

#[test]
fn word_right_skips_multiple_spaces() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);
    let lines = ["a   b"];
    move_editor(&mut scroller, Action::ScrollWordRight, &lines);
    assert_eq!(scroller.horizontal(), 1);
    move_editor(&mut scroller, Action::ScrollWordRight, &lines);
    assert_eq!(scroller.horizontal(), 5);
}

#[test]
fn word_right_wraps_to_next_line_start() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);
    let lines = ["hello", "world"];
    scroller.set_cursor(Position {
        vertical: 0,
        horizontal: 5,
    });
    move_editor(&mut scroller, Action::ScrollWordRight, &lines);
    assert_eq!(
        scroller.cursor(),
        Position {
            vertical: 1,
            horizontal: 0,
        }
    );
}

#[test]
fn word_left_wraps_to_prev_line_end() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);
    let lines = ["hello", "world"];
    scroller.set_cursor(Position {
        vertical: 1,
        horizontal: 0,
    });
    move_editor(&mut scroller, Action::ScrollWordLeft, &lines);
    assert_eq!(
        scroller.cursor(),
        Position {
            vertical: 0,
            horizontal: 5,
        }
    );
}

#[test]
fn vertical_move_restores_preferred_column_on_long_lines() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);
    let lines = [
        "x".repeat(200),
        "short".to_string(),
        String::new(),
        "y".repeat(200),
    ];
    let lines = lines.iter().map(String::as_str).collect::<Vec<_>>();

    for _ in 0..200 {
        move_editor(&mut scroller, Action::ScrollRight, &lines);
    }
    assert_eq!(scroller.cursor().horizontal, 200);

    move_editor(&mut scroller, Action::ScrollDown, &lines);
    assert_eq!(scroller.cursor().horizontal, 5);

    move_editor(&mut scroller, Action::ScrollDown, &lines);
    assert_eq!(scroller.cursor().horizontal, 0);

    move_editor(&mut scroller, Action::ScrollDown, &lines);
    assert_eq!(scroller.cursor().horizontal, 200);
}

#[test]
fn move_down_to_empty_line_clamps_column_and_scrolls_back() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);
    let lines = [
        "x".repeat(200),
        "## Notes".to_string(),
        String::new(),
        "y".repeat(200),
    ];
    let lines = lines.iter().map(String::as_str).collect::<Vec<_>>();

    for _ in 0..200 {
        move_editor(&mut scroller, Action::ScrollRight, &lines);
    }

    let inner_width = 40u16;
    let gutter = format!("{:<6} ", lines.len()).chars().count() as u16;
    let _ = scroller.get_scroll(0, 10, inner_width, gutter);

    move_editor(&mut scroller, Action::ScrollDown, &lines);
    assert_eq!(scroller.cursor().horizontal, 8);
    let s1 = scroller.get_scroll(1, 10, inner_width, gutter);
    assert_eq!(s1.horizontal, 8 + 7 - 3);

    move_editor(&mut scroller, Action::ScrollDown, &lines);
    assert_eq!(scroller.cursor().horizontal, 0);
    let s2 = scroller.get_scroll(2, 10, inner_width, gutter);
    assert_eq!(s2.horizontal, 7 - 3);

    let visible = gutter + scroller.cursor().horizontal as u16 - s2.horizontal;
    assert!(visible < inner_width);
}
