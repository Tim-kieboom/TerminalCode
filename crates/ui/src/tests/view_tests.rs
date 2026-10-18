use crate::view::rows_scroll;

#[test]
fn nothing_scrolls_while_the_selected_row_is_near_the_top() {
    assert_eq!(rows_scroll(Some(0), 100, 10), 0);
    assert_eq!(rows_scroll(Some(4), 100, 10), 0);
}

#[test]
fn the_pane_is_centred_on_the_selected_row_further_down() {
    assert_eq!(rows_scroll(Some(5), 100, 10), 0);
    assert_eq!(rows_scroll(Some(6), 100, 10), 1);
    assert_eq!(rows_scroll(Some(50), 100, 10), 45);
}

#[test]
fn the_last_row_stays_at_the_bottom() {
    assert_eq!(rows_scroll(Some(99), 100, 10), 90);
    assert_eq!(rows_scroll(Some(95), 100, 10), 90);
}

#[test]
fn few_rows_do_not_scroll_and_no_selection_stays_at_the_top() {
    assert_eq!(rows_scroll(Some(3), 5, 10), 0);
    assert_eq!(rows_scroll(None, 100, 10), 0);
    assert_eq!(rows_scroll(Some(50), 100, 0), 0, "a pane with no height");
}

#[test]
fn the_selected_row_is_always_among_the_rows_shown() {
    for len in [1usize, 2, 9, 10, 11, 57] {
        for height in [1usize, 2, 5, 10, 30] {
            for selected in 0..len {
                let top = rows_scroll(Some(selected), len, height);
                assert!(
                    (top..top + height).contains(&selected),
                    "{selected} of {len} in {height}: top {top}"
                );
            }
        }
    }
}
