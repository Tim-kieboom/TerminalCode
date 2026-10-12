use super::super::text_layout::*;

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
    assert_eq!(
        drawn(&visible_cells("\tx", 0, 20, TabWidth::default())),
        "    x"
    );
    assert_eq!(
        drawn(&visible_cells("ab\tx", 0, 20, TabWidth::default())),
        "ab  x"
    );
    assert_eq!(
        drawn(&visible_cells("abcd\tx", 0, 20, TabWidth::default())),
        "abcd    x"
    );
}

#[test]
fn display_column_accounts_for_tabs_and_wide_characters() {
    assert_eq!(display_column("a\tb", 1, TabWidth::default()), 1);
    assert_eq!(display_column("a\tb", 2, TabWidth::default()), 4);
    assert_eq!(display_column("\u{3053}a", 1, TabWidth::default()), 2);
    assert_eq!(display_column("\u{3053}a", 2, TabWidth::default()), 3);
}

#[test]
fn display_column_past_the_end_is_the_full_width() {
    assert_eq!(display_column("ab", 10, TabWidth::default()), 2);
}

#[test]
fn window_clips_on_the_right() {
    assert_eq!(
        drawn(&visible_cells("abcdef", 0, 3, TabWidth::default())),
        "abc"
    );
}

#[test]
fn window_scrolls_from_the_left() {
    let cells = visible_cells("abcdef", 2, 3, TabWidth::default());

    assert_eq!(drawn(&cells), "cde");
    assert_eq!(cells[0].grapheme, 2);
}

#[test]
fn wide_character_cut_by_the_left_edge_becomes_a_space() {
    let cells = visible_cells("\u{3053}b", 1, 5, TabWidth::default());

    assert_eq!(drawn(&cells), " b");
    assert_eq!(cells[0].grapheme, 0);
}

#[test]
fn wide_character_cut_by_the_right_edge_becomes_a_space() {
    assert_eq!(
        drawn(&visible_cells("a\u{3053}", 0, 2, TabWidth::default())),
        "a "
    );
}

#[test]
fn grapheme_clusters_stay_one_cell() {
    let cells = visible_cells("e\u{301}x", 0, 10, TabWidth::default());

    assert_eq!(cells.len(), 2);
    assert_eq!(cells[1].grapheme, 1);
}

#[test]
fn zero_width_window_has_no_cells() {
    assert!(visible_cells("abc", 0, 0, TabWidth::default()).is_empty());
}

#[test]
fn clicking_the_left_half_of_a_cell_lands_before_it_and_the_right_half_after() {
    // "ab": a is column 0, b is column 1.
    assert_eq!(column_at_display("ab", 0, TabWidth::default()), 0);
    assert_eq!(column_at_display("ab", 1, TabWidth::default()), 1);
    assert_eq!(column_at_display("ab", 2, TabWidth::default()), 2);
}

#[test]
fn clicking_a_wide_character_picks_the_nearest_edge() {
    // The wide character covers display columns 0 and 1.
    assert_eq!(column_at_display("\u{3053}a", 0, TabWidth::default()), 0);
    assert_eq!(column_at_display("\u{3053}a", 1, TabWidth::default()), 1);
    assert_eq!(column_at_display("\u{3053}a", 2, TabWidth::default()), 1);
    assert_eq!(column_at_display("\u{3053}a", 3, TabWidth::default()), 2);
}

#[test]
fn clicking_a_tab_picks_the_nearest_edge() {
    // The tab covers display columns 0..4.
    assert_eq!(column_at_display("\tx", 0, TabWidth::default()), 0);
    assert_eq!(column_at_display("\tx", 1, TabWidth::default()), 0);
    assert_eq!(column_at_display("\tx", 2, TabWidth::default()), 1);
    assert_eq!(column_at_display("\tx", 3, TabWidth::default()), 1);
    assert_eq!(column_at_display("\tx", 4, TabWidth::default()), 1);
}

#[test]
fn clicking_past_the_end_of_a_line_lands_at_its_end() {
    assert_eq!(column_at_display("abc", 50, TabWidth::default()), 3);
    assert_eq!(column_at_display("", 5, TabWidth::default()), 0);
}

#[test]
fn clicking_inside_a_grapheme_cluster_never_splits_it() {
    let flag = "\u{1F1F3}\u{1F1F1}";

    assert_eq!(
        column_at_display(&format!("{flag}x"), 0, TabWidth::default()),
        0
    );
    assert_eq!(
        column_at_display(&format!("{flag}x"), 2, TabWidth::default()),
        1
    );
}

#[test]
fn column_at_display_inverts_display_column_on_boundaries() {
    let text = "a\tb\u{3053}c";
    for column in 0..=5 {
        let display = display_column(text, column, TabWidth::default());

        assert_eq!(
            column_at_display(text, display, TabWidth::default()),
            column
        );
    }
}

fn width(columns: usize) -> TabWidth {
    TabWidth::new(columns).unwrap()
}

#[test]
fn a_tab_width_must_be_between_one_and_sixteen() {
    assert_eq!(TabWidth::new(0), None);
    assert_eq!(TabWidth::new(17), None);
    assert_eq!(TabWidth::new(1).map(TabWidth::get), Some(1));
    assert_eq!(TabWidth::new(16).map(TabWidth::get), Some(16));
    assert_eq!(TabWidth::default().get(), 4);
}

#[test]
fn tabs_expand_to_the_stops_of_the_width_given() {
    assert_eq!(drawn(&visible_cells("\tx", 0, 20, width(2))), "  x");
    assert_eq!(drawn(&visible_cells("a\tx", 0, 20, width(2))), "a x");
    assert_eq!(drawn(&visible_cells("\tx", 0, 20, width(8))), "        x");
    assert_eq!(
        drawn(&visible_cells("abc\tx", 0, 20, width(8))),
        "abc     x"
    );
    assert_eq!(drawn(&visible_cells("\tx", 0, 20, width(1))), " x");
}

#[test]
fn display_column_and_column_at_display_follow_the_width() {
    assert_eq!(display_column("a\tb", 2, width(8)), 8);
    assert_eq!(display_column("a\tb", 2, width(2)), 2);
    // A click in the second half of an 8 wide tab lands after it.
    assert_eq!(column_at_display("\tb", 5, width(8)), 1);
    assert_eq!(column_at_display("\tb", 2, width(8)), 0);
}

#[test]
fn a_tab_cut_by_the_edge_of_the_window_shows_the_part_that_is_visible() {
    assert_eq!(drawn(&visible_cells("\tx", 5, 6, width(8))), "   x");
}
