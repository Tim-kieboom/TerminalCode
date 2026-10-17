use ratatui::layout::Rect;

use super::*;

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
        document: crate::ids::DocumentId::for_test(0),
        file: file.to_owned(),
        position: Position::new(line, 0),
        severity,
        message: String::new(),
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
