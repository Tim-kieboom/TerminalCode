use super::{CursorScroller, Position, ScrollMode};
use crate::keybinds::Action;

#[test]
fn new_starts_at_origin() {
    let scroller = CursorScroller::new(ScrollMode::List);
    assert_eq!(scroller.position(), Position::default());
}

#[test]
fn move_cursor_is_noop_for_empty_list() {
    let mut scroller = CursorScroller::new(ScrollMode::List);
    scroller.move_cursor(Action::ScrollDown, 0, 0);
    scroller.move_cursor(Action::ScrollBottom, 0, 0);
    assert_eq!(scroller.position(), Position::default());
}

#[test]
fn scroll_down_clamps_at_last_item() {
    let mut scroller = CursorScroller::new(ScrollMode::List);
    for _ in 0..10 {
        scroller.move_cursor(Action::ScrollDown, 5, 0);
    }
    assert_eq!(scroller.position().vertical, 4);
}

#[test]
fn scroll_up_saturates_at_zero() {
    let mut scroller = CursorScroller::new(ScrollMode::List);
    scroller.move_cursor(Action::ScrollDown, 5, 0);
    scroller.move_cursor(Action::ScrollUp, 5, 0);
    scroller.move_cursor(Action::ScrollUp, 5, 0);
    assert_eq!(scroller.position().vertical, 0);
}

#[test]
fn scroll_top_and_bottom() {
    let mut scroller = CursorScroller::new(ScrollMode::List);
    scroller.move_cursor(Action::ScrollDown, 5, 0);
    scroller.move_cursor(Action::ScrollBottom, 5, 0);
    assert_eq!(scroller.position().vertical, 4);

    scroller.move_cursor(Action::ScrollTop, 5, 0);
    assert_eq!(scroller.position().vertical, 0);
}

#[test]
fn scroll_right_clamps_at_width() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);
    for _ in 0..10 {
        scroller.move_cursor(Action::ScrollRight, 5, 3);
    }
    assert_eq!(scroller.position().horizontal, 3);
}

#[test]
fn scroll_page_down_clamps_at_last() {
    let mut scroller = CursorScroller::new(ScrollMode::List);
    scroller.move_cursor(Action::ScrollPageDown, 100, 0);
    assert_eq!(scroller.position().vertical, 10);
    scroller.move_cursor(Action::ScrollPageDown, 100, 0);
    scroller.move_cursor(Action::ScrollPageDown, 100, 0);
    assert_eq!(scroller.position().vertical, 30);
    scroller.move_cursor(Action::ScrollPageDown, 25, 0);
    assert_eq!(scroller.position().vertical, 24);
}

#[test]
fn clamp_column_caps_horizontal() {
    let mut scroller = CursorScroller::new(ScrollMode::TextEditor);
    scroller.set_position(Position {
        vertical: 0,
        horizontal: 10,
    });
    scroller.clamp_column(4);
    assert_eq!(scroller.position().horizontal, 4);
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
    scroller.clamp_column(1);
    let scroll = scroller.get_scroll(1, 10, 20, 7);
    assert_eq!(scroll.horizontal, 1 + 7 - 3);
}
