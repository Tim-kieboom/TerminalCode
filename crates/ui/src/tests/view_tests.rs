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

use crate::view::{
    ViewButton, ViewLine, ViewNode, ViewRow, button_at, button_cells, button_lines, input_column_at,
};
use ratatui::layout::Rect;

fn button(label: &str, enabled: bool) -> ViewButton {
    ViewButton {
        id: label.to_lowercase().into(),
        label: label.into(),
        enabled,
    }
}

fn rows(count: usize) -> ViewNode {
    ViewNode::Rows {
        rows: (0..count)
            .map(|n| ViewRow {
                spans: vec![ViewLine {
                    text: format!("row {n}").into(),
                    slot: "".into(),
                }],
            })
            .collect(),
        selected: Some(0),
    }
}

fn input(active: bool) -> ViewNode {
    ViewNode::Input {
        prompt: "> ".into(),
        value: "msg".into(),
        cursor: 3,
        active,
    }
}

fn column() -> ViewNode {
    ViewNode::Column(vec![
        ViewNode::Buttons(vec![button("Commit", true), button("Push", false)]),
        rows(50),
        input(false),
    ])
}

#[test]
fn a_view_that_is_not_a_column_is_its_own_part() {
    let area = Rect::new(2, 3, 20, 8);
    let view = rows(3);
    assert_eq!(view.leaves(area), vec![(area, &view)]);
}

#[test]
fn a_column_gives_the_buttons_and_the_input_a_row_and_the_rows_the_rest() {
    let area = Rect::new(2, 3, 20, 10);
    let view = column();
    let leaves = view.leaves(area);
    let rects: Vec<Rect> = leaves.iter().map(|(rect, _)| *rect).collect();
    assert_eq!(
        rects,
        [
            Rect::new(2, 3, 20, 1),
            Rect::new(2, 4, 20, 8),
            Rect::new(2, 12, 20, 1)
        ]
    );
}

#[test]
fn two_flexible_parts_share_what_is_left_and_the_odd_row_goes_to_the_first() {
    let view = ViewNode::Column(vec![rows(9), rows(9), input(true)]);
    let heights: Vec<u16> = view
        .leaves(Rect::new(0, 0, 10, 10))
        .iter()
        .map(|(rect, _)| rect.height)
        .collect();
    assert_eq!(heights, [5, 4, 1]);
}

#[test]
fn a_column_in_a_pane_too_short_for_it_drops_the_last_parts_and_never_overflows() {
    let view = column();
    for height in 0..6u16 {
        let area = Rect::new(0, 0, 40, height);
        let leaves = view.leaves(area);
        let total: u16 = leaves.iter().map(|(rect, _)| rect.height).sum();
        assert!(total <= height, "{height}: {total}");
        assert!(
            leaves
                .iter()
                .all(|(rect, _)| rect.bottom() <= area.bottom())
        );
    }
    // Two rows: the buttons and the rows' last row; the input does not fit.
    let two = column().leaves(Rect::new(0, 0, 40, 2)).len();
    assert_eq!(two, 1 + 1);
}

#[test]
fn a_column_inside_a_column_is_not_drawn() {
    let view = ViewNode::Column(vec![ViewNode::Column(vec![rows(3)]), input(true)]);
    let leaves = view.leaves(Rect::new(0, 0, 10, 5));
    assert_eq!(leaves.len(), 1);
}

#[test]
fn only_an_active_line_makes_the_keys_the_plugins() {
    assert!(input(true).is_input());
    assert!(!input(false).is_input());
    assert!(ViewNode::Column(vec![rows(1), input(true)]).is_input());
    assert!(!column().is_input(), "its line is idle");
    assert!(!rows(1).is_input());
}

#[test]
fn buttons_are_a_space_a_label_a_space_and_a_gap() {
    let buttons = [button("Commit", true), button("Push", true)];
    // " Commit " is 8 wide, then a gap, then " Push " from column 9.
    assert_eq!(button_cells(&buttons, 40), [(0, 0, 8), (0, 9, 6)]);
    assert_eq!(button_at(&buttons, 40, 0, 0), Some(0));
    assert_eq!(button_at(&buttons, 40, 0, 7), Some(0));
    assert_eq!(button_at(&buttons, 40, 0, 8), None, "the gap");
    assert_eq!(button_at(&buttons, 40, 0, 9), Some(1));
    assert_eq!(button_at(&buttons, 40, 0, 14), Some(1));
    assert_eq!(button_at(&buttons, 40, 0, 15), None);
    assert_eq!(button_at(&buttons, 40, 1, 2), None, "no second line");
}

#[test]
fn a_click_in_a_line_to_type_into_is_a_character_of_its_text() {
    // "> msg": the prompt is 2 wide.
    assert_eq!(input_column_at("> ", "msg", 0), 0, "on the prompt");
    assert_eq!(input_column_at("> ", "msg", 2), 0);
    assert_eq!(input_column_at("> ", "msg", 3), 1);
    assert_eq!(input_column_at("> ", "msg", 4), 2);
    assert_eq!(input_column_at("> ", "msg", 5), 3, "past the end");
    assert_eq!(input_column_at("> ", "msg", 40), 3);
    assert_eq!(input_column_at("> ", "", 9), 0);
}

#[test]
fn a_wide_character_takes_two_columns_of_the_click() {
    // "é" is one column, "世" is two.
    assert_eq!(input_column_at("", "a世b", 1), 1);
    assert_eq!(input_column_at("", "a世b", 2), 1, "the second half of 世");
    assert_eq!(input_column_at("", "a世b", 3), 2);
}

#[test]
fn buttons_that_do_not_fit_go_on_the_next_line_and_the_column_gives_them_the_rows() {
    let buttons = [
        button("Commit", true),
        button("Stage All", true),
        button("Unstage All", true),
        button("Fetch", true),
    ];
    // 8, 11, 13 and 7 wide, with a column between: 8+1+11 = 20 fits in 22, the
    // third does not and starts the second line, the fourth fits beside it.
    assert_eq!(
        button_cells(&buttons, 22),
        [(0, 0, 8), (0, 9, 11), (1, 0, 13), (1, 14, 7)]
    );
    assert_eq!(button_lines(&buttons, 22), 2);
    assert_eq!(button_lines(&buttons, 200), 1);
    assert_eq!(button_lines(&[], 22), 0);
    assert_eq!(button_at(&buttons, 22, 1, 0), Some(2));
    assert_eq!(button_at(&buttons, 22, 1, 15), Some(3));
    assert_eq!(
        button_at(&buttons, 22, 0, 20),
        None,
        "the first line ends at 20"
    );

    let view = ViewNode::Column(vec![
        ViewNode::Buttons(buttons.to_vec()),
        rows(9),
        input(true),
    ]);
    let wide = view.leaves(Rect::new(0, 0, 200, 10));
    let narrow = view.leaves(Rect::new(0, 0, 22, 10));
    assert_eq!((wide[0].0.height, wide[1].0.height), (1, 8));
    assert_eq!((narrow[0].0.height, narrow[1].0.height), (2, 7));
}

#[test]
fn a_button_wider_than_the_row_is_alone_on_a_line() {
    let buttons = [button("A very long label indeed", true), button("B", true)];
    let cells = button_cells(&buttons, 10);
    assert_eq!(cells[0], (0, 0, 26));
    assert_eq!(cells[1].0, 1);
}
