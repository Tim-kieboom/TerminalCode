use super::*;

fn drawn(cells: &[Cell]) -> String {
    cells.iter().map(|cell| cell.text.as_str()).collect()
}

#[test]
fn digits_counts_decimal_digits() {
    assert_eq!(digits(0), 1);
    assert_eq!(digits(9), 1);
    assert_eq!(digits(10), 2);
    assert_eq!(digits(12345), 5);
}

#[test]
fn tabs_expand_to_the_next_tab_stop() {
    assert_eq!(drawn(&visible_cells("\tx", 0, 20)), "    x");
    assert_eq!(drawn(&visible_cells("ab\tx", 0, 20)), "ab  x");
    assert_eq!(drawn(&visible_cells("abcd\tx", 0, 20)), "abcd    x");
}

#[test]
fn display_column_accounts_for_tabs_and_wide_characters() {
    assert_eq!(display_column("a\tb", 1), 1);
    assert_eq!(display_column("a\tb", 2), 4);
    assert_eq!(display_column("\u{3053}a", 1), 2);
    assert_eq!(display_column("\u{3053}a", 2), 3);
}

#[test]
fn display_column_past_the_end_is_the_full_width() {
    assert_eq!(display_column("ab", 10), 2);
}

#[test]
fn window_clips_on_the_right() {
    assert_eq!(drawn(&visible_cells("abcdef", 0, 3)), "abc");
}

#[test]
fn window_scrolls_from_the_left() {
    let cells = visible_cells("abcdef", 2, 3);

    assert_eq!(drawn(&cells), "cde");
    assert_eq!(cells[0].grapheme, 2);
}

#[test]
fn wide_character_cut_by_the_left_edge_becomes_a_space() {
    let cells = visible_cells("\u{3053}b", 1, 5);

    assert_eq!(drawn(&cells), " b");
    assert_eq!(cells[0].grapheme, 0);
}

#[test]
fn wide_character_cut_by_the_right_edge_becomes_a_space() {
    assert_eq!(drawn(&visible_cells("a\u{3053}", 0, 2)), "a ");
}

#[test]
fn grapheme_clusters_stay_one_cell() {
    let cells = visible_cells("e\u{301}x", 0, 10);

    assert_eq!(cells.len(), 2);
    assert_eq!(cells[1].grapheme, 1);
}

#[test]
fn zero_width_window_has_no_cells() {
    assert!(visible_cells("abc", 0, 0).is_empty());
}
