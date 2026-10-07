use crate::buffer::position::Position;
use crate::buffer::selection::{Selection, Selections};

#[test]
fn cursor_is_an_empty_selection() {
    let selection = Selection::cursor(Position::new(2, 3));

    assert!(selection.is_empty());
    assert_eq!(selection.start(), selection.end());
}

#[test]
fn start_and_end_ignore_direction() {
    let forward = Selection::new(Position::new(1, 4), Position::new(3, 0));
    let backward = Selection::new(Position::new(3, 0), Position::new(1, 4));

    assert_eq!(forward.start(), Position::new(1, 4));
    assert_eq!(forward.end(), Position::new(3, 0));
    assert_eq!(backward.start(), forward.start());
    assert_eq!(backward.end(), forward.end());
    assert_eq!(backward.head(), Position::new(1, 4));
}

#[test]
fn desired_column_is_kept_separately_from_the_head() {
    let selection = Selection::cursor(Position::new(0, 2)).with_desired_column(Some(10));

    assert_eq!(selection.head(), Position::new(0, 2));
    assert_eq!(selection.desired_column(), Some(10));
}

#[test]
fn default_selections_hold_one_cursor_at_the_origin() {
    let selections = Selections::default();

    assert_eq!(selections.len(), 1);
    assert_eq!(selections.primary().head(), Position::default());
}
