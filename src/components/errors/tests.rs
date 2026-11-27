use ratatui::layout::Rect;

use super::*;
use crate::ui::ComponentKind;

/// A view drawn into `height` rows with `count` rows of problems.
fn view(height: u16, count: usize) -> ErrorsView {
    let mut view = ErrorsView::default();
    let area = Rect::new(0, 10, 80, height + 1);
    view.prepare(area, Rect::new(0, 11, 80, height), count);
    view
}

#[test]
fn the_selection_stays_among_the_rows() {
    let mut view = view(5, 3);

    view.apply(ErrorsCommand::Down, 3);
    view.apply(ErrorsCommand::Down, 3);
    view.apply(ErrorsCommand::Down, 3);
    assert_eq!(view.selected(), 2);

    view.apply(ErrorsCommand::First, 3);
    assert_eq!(view.selected(), 0);
    view.apply(ErrorsCommand::Up, 3);
    assert_eq!(view.selected(), 0);
}

#[test]
fn a_page_moves_by_the_rows_shown_less_one() {
    let mut view = view(5, 50);

    view.apply(ErrorsCommand::PageDown, 50);
    assert_eq!(view.selected(), 4);

    view.apply(ErrorsCommand::Last, 50);
    assert_eq!(view.selected(), 49);
    view.apply(ErrorsCommand::PageUp, 50);
    assert_eq!(view.selected(), 45);
}

#[test]
fn the_selection_is_kept_in_view_after_a_move() {
    let mut view = view(5, 50);

    view.apply(ErrorsCommand::Last, 50);
    view.prepare(Rect::new(0, 10, 80, 6), Rect::new(0, 11, 80, 5), 50);

    assert_eq!(view.scroll(), 45);
}

#[test]
fn the_wheel_scrolls_without_moving_the_selection_and_stops_at_the_end() {
    let mut view = view(5, 50);

    view.scroll_by(3, 50);
    view.prepare(Rect::new(0, 10, 80, 6), Rect::new(0, 11, 80, 5), 50);
    assert_eq!((view.scroll(), view.selected()), (3, 0));

    view.scroll_by(1000, 50);
    assert_eq!(view.scroll(), 45);
    view.scroll_by(-1000, 50);
    assert_eq!(view.scroll(), 0);
}

#[test]
fn a_list_that_got_shorter_pulls_the_selection_and_scroll_back() {
    let mut view = view(5, 50);
    view.apply(ErrorsCommand::Last, 50);
    view.prepare(Rect::new(0, 10, 80, 6), Rect::new(0, 11, 80, 5), 50);

    view.prepare(Rect::new(0, 10, 80, 6), Rect::new(0, 11, 80, 5), 3);

    assert_eq!(view.selected(), 2);
    assert_eq!(view.scroll(), 0);
}

#[test]
fn a_click_selects_the_row_under_it_only_when_there_is_one() {
    let mut view = view(5, 3);

    assert!(view.select_at(4, 12, 3));
    assert_eq!(view.selected(), 1);
    assert!(!view.select_at(4, 14, 3), "the fourth row does not exist");
    assert!(!view.select_at(4, 10, 3), "the frame is not a row");
}

#[test]
fn the_rows_sort_errors_first_then_by_file_and_place() {
    let row = |file: &str, line: usize, severity| ErrorRow {
        target: super::ErrorTarget::Document(crate::ids::DocumentId::new(0)),
        file: file.to_owned(),
        position: Position::new(line, 0),
        severity,
        message: String::new(),
        code: None,
        version: None,
        id: ProblemId::default(),
    };
    let mut rows = vec![
        row("b.rs", 0, Severity::Warning),
        row("b.rs", 5, Severity::Error),
        row("a.rs", 9, Severity::Hint),
        row("b.rs", 1, Severity::Error),
        row("a.rs", 2, Severity::Warning),
    ];

    sort(&mut rows);

    let order: Vec<_> = rows
        .iter()
        .map(|row| (row.file.as_str(), row.position.line))
        .collect();
    assert_eq!(
        order,
        [
            ("b.rs", 1),
            ("b.rs", 5),
            ("a.rs", 2),
            ("b.rs", 0),
            ("a.rs", 9)
        ]
    );
}

/// A problem of an open file with the text at `version`.
fn problem(file: &str, line: usize, message: &str, version: u64) -> ErrorRow {
    ErrorRow::new(
        ErrorTarget::Document(crate::ids::DocumentId::new(0)),
        None,
        file,
        None,
        Position::new(line, 2),
        Severity::Error,
        message,
    )
    .with_origin(Some("E1"), Some(version))
}

/// A list as `sort` makes it, ids included.
fn listed(mut rows: Vec<ErrorRow>) -> Vec<ErrorRow> {
    sort(&mut rows);
    rows
}

/// A view with `rows` and the row at `index` selected.
fn selecting(rows: &[ErrorRow], index: usize) -> ErrorsView {
    let mut view = view(5, rows.len());
    view.reconcile(rows);
    view.apply(ErrorsCommand::First, rows.len());
    for _ in 0..index {
        view.apply(ErrorsCommand::Down, rows.len());
    }
    view.pin(rows);
    view
}

#[test]
fn what_is_copied_is_the_place_and_the_message() {
    let row = problem("src/a.rs", 11, "mismatched types", 1);

    assert_eq!(
        row.copy_text(CopyKind::Full),
        "src/a.rs:12:3 mismatched types"
    );
    assert_eq!(row.copy_text(CopyKind::Message), "mismatched types");
    assert_eq!(row.copy_text(CopyKind::Place), "src/a.rs:12:3");
}

#[test]
fn equal_problems_of_one_file_get_ids_of_their_own_and_the_same_ones_again() {
    let first = listed(vec![
        problem("a.rs", 1, "unused", 1),
        problem("a.rs", 9, "unused", 1),
        problem("a.rs", 4, "other", 1),
    ]);
    let again = listed(vec![
        problem("a.rs", 2, "unused", 2),
        problem("a.rs", 10, "unused", 2),
        problem("a.rs", 5, "other", 2),
    ]);

    let ids = |rows: &[ErrorRow]| rows.iter().map(|row| row.id).collect::<Vec<_>>();
    assert_eq!(ids(&first), ids(&again), "the place is not in the id");
    assert_ne!(first[0].id, first[1].id);
    assert_ne!(first[1].id, first[2].id);
}

#[test]
fn nothing_is_selected_before_there_are_rows_and_then_the_first_is() {
    let mut view = ErrorsView::default();
    view.reconcile(&[]);
    assert_eq!(view.selected_row(&[]), None);

    let rows = listed(vec![problem("a.rs", 1, "x", 1)]);
    view.reconcile(&rows);

    assert_eq!(view.selected_row(&rows), Some(&rows[0]));
}

#[test]
fn the_selection_follows_its_problem_when_the_list_changes() {
    let rows = listed(vec![
        problem("a.rs", 1, "one", 1),
        problem("a.rs", 3, "two", 1),
    ]);
    let mut view = selecting(&rows, 1);
    // A new problem sorts in before it, and an edit above moved both down a line.
    let after = listed(vec![
        problem("a.rs", 0, "new", 2),
        problem("a.rs", 2, "one", 2),
        problem("a.rs", 4, "two", 2),
    ]);

    view.reconcile(&after);

    let selected = view.selected_row(&after).expect("still selected");
    assert_eq!(selected.message, "two");
    assert_eq!(view.selected(), 2, "the cursor went with it");
    assert!(view.is_selected(2));
}

#[test]
fn a_problem_that_is_gone_leaves_nothing_selected_until_a_new_choice() {
    let rows = listed(vec![
        problem("a.rs", 1, "one", 1),
        problem("a.rs", 3, "two", 1),
    ]);
    let mut view = selecting(&rows, 1);
    let after = listed(vec![problem("a.rs", 1, "one", 2)]);

    view.reconcile(&after);

    assert_eq!(view.selected_row(&after), None);
    assert!(!view.is_selected(0));
    view.reconcile(&rows);
    assert_eq!(
        view.selected_row(&rows),
        None,
        "it does not come back by itself"
    );

    view.pin(&after);
    assert_eq!(view.selected_row(&after), Some(&after[0]));
}

#[test]
fn fixing_the_first_of_two_equal_problems_drops_the_selection_when_the_server_says_so() {
    let rows = listed(vec![
        problem("a.rs", 1, "unused", 1),
        problem("a.rs", 9, "unused", 1),
    ]);
    let mut view = selecting(&rows, 0);
    // The edit that fixed it: the view sees the new text, the server has not spoken yet.
    let edited = listed(vec![
        problem("a.rs", 1, "unused", 2),
        problem("a.rs", 9, "unused", 2),
    ]);
    view.reconcile(&edited);
    // The server speaks: one problem is left, and it takes the first one's number.
    let said = listed(vec![problem("a.rs", 9, "unused", 2)]);

    view.reconcile(&said);

    assert_eq!(view.selected_row(&said), None);
}

#[test]
fn an_edit_above_the_problem_does_not_drop_the_selection() {
    let rows = listed(vec![problem("a.rs", 5, "x", 1)]);
    let mut view = selecting(&rows, 0);
    // The text changed (version 2) and the problem is a line lower.
    let moved = listed(vec![problem("a.rs", 6, "x", 2)]);

    view.reconcile(&moved);

    assert_eq!(view.selected_row(&moved), Some(&moved[0]));
}

#[test]
fn a_file_that_is_not_open_follows_the_id_whatever_the_line() {
    let closed = |line| {
        ErrorRow::new(
            ErrorTarget::File("/p/a.rs".into()),
            Some(std::path::Path::new("/p/a.rs")),
            "",
            None,
            Position::new(line, 0),
            Severity::Error,
            "x",
        )
    };
    let rows = listed(vec![closed(4)]);
    let mut view = selecting(&rows, 0);
    let moved = listed(vec![closed(7)]);

    view.reconcile(&moved);

    assert_eq!(view.selected_row(&moved), Some(&moved[0]));
}

#[test]
fn the_button_is_at_the_right_end_of_the_title_row_when_there_is_room() {
    use crate::ui::layout::Placement;
    let placement = |width| Placement::new(ComponentKind::Errors, Rect::new(0, 10, width, 6));

    let wide = copy_button_rect(&placement(40)).expect("room");
    assert_eq!((wide.y, wide.right()), (10, 39));
    assert_eq!(
        copy_button_rect(&placement(12)),
        None,
        "the title is in the way"
    );
}
