use crate::components::list_cursor::{ListCursor, Step, revealed, stepped};

#[test]
fn steps_stop_at_both_ends() {
    assert_eq!(stepped(0, Step::Up, 5, 3), 0);
    assert_eq!(stepped(4, Step::Down, 5, 3), 4);
    assert_eq!(stepped(1, Step::PageUp, 5, 3), 0);
    assert_eq!(stepped(3, Step::PageDown, 5, 3), 4);
    assert_eq!(stepped(2, Step::First, 5, 3), 0);
    assert_eq!(stepped(2, Step::Last, 5, 3), 4);
    assert_eq!(stepped(0, Step::Last, 0, 3), 0);
}

#[test]
fn reveal_scrolls_just_far_enough() {
    assert_eq!(revealed(2, 0, 5), 0);
    assert_eq!(revealed(7, 0, 5), 3);
    assert_eq!(revealed(1, 4, 5), 1);
    assert_eq!(revealed(9, 2, 0), 2, "no lines: the scroll stays");
}

#[test]
fn a_selection_moved_past_the_view_is_followed_by_the_scroll() {
    let mut cursor = ListCursor::default();
    for _ in 0..20 {
        cursor.step(Step::Down, 30, 5);
        cursor.reveal(14);
    }
    assert_eq!(cursor.selected(), 20);
    assert_eq!(cursor.scroll(), 7);
}

#[test]
fn scrolling_does_not_move_the_selection_and_stops_at_the_end() {
    let mut cursor = ListCursor::default();
    cursor.scroll_by(100, 30, 10);
    assert_eq!((cursor.selected(), cursor.scroll()), (0, 20));
    cursor.scroll_by(-100, 30, 10);
    assert_eq!(cursor.scroll(), 0);
}

#[test]
fn clamp_brings_a_stale_cursor_back_inside_a_shorter_list() {
    let mut cursor = ListCursor::default();
    cursor.select(25, 30);
    cursor.scroll_by(20, 30, 5);
    cursor.clamp(10, 4);
    assert_eq!((cursor.selected(), cursor.scroll()), (9, 6));
}

#[test]
fn shift_and_select_row_stay_inside_the_list() {
    let mut cursor = ListCursor::default();
    cursor.shift(50, 8);
    assert_eq!(cursor.selected(), 7);
    cursor.shift(-50, 8);
    assert_eq!(cursor.selected(), 0);
    assert!(cursor.select_row(3, 8));
    assert_eq!(cursor.selected(), 3);
    assert!(!cursor.select_row(8, 8));
    assert_eq!(cursor.selected(), 3);
}
