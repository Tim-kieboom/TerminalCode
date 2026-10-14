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

fn cursor(line: usize, column: usize) -> Selection {
    Selection::cursor(Position::new(line, column))
}

#[test]
fn a_list_is_sorted_and_keeps_its_primary() {
    let list = Selections::from_items(vec![cursor(3, 0), cursor(1, 0), cursor(2, 0)], 0);

    let starts: Vec<_> = list.iter().map(Selection::start).collect();
    assert_eq!(
        starts,
        vec![
            Position::new(1, 0),
            Position::new(2, 0),
            Position::new(3, 0)
        ]
    );
    assert_eq!(list.primary().head(), Position::new(3, 0));
    assert_eq!(list.primary_index(), 2);
}

#[test]
fn cursors_at_the_same_place_become_one() {
    let list = Selections::from_items(vec![cursor(1, 2), cursor(1, 2)], 1);

    assert!(list.is_single());
}

#[test]
fn overlapping_selections_are_joined_and_the_primary_survives() {
    let first = Selection::new(Position::new(0, 0), Position::new(0, 5));
    let second = Selection::new(Position::new(0, 3), Position::new(0, 8));
    let apart = cursor(2, 0);

    let list = Selections::from_items(vec![first, apart, second], 2);

    assert_eq!(list.len(), 2);
    let joined = list.primary();
    assert_eq!(joined.start(), Position::new(0, 0));
    assert_eq!(joined.end(), Position::new(0, 8));
}

#[test]
fn a_cursor_on_the_edge_of_a_selection_is_joined_but_two_touching_selections_are_not() {
    let selected = Selection::new(Position::new(0, 0), Position::new(0, 3));

    let with_cursor = Selections::from_items(vec![selected, cursor(0, 3)], 0);
    let touching = Selections::from_items(
        vec![
            selected,
            Selection::new(Position::new(0, 3), Position::new(0, 6)),
        ],
        0,
    );

    assert!(with_cursor.is_single());
    assert_eq!(touching.len(), 2);
}

#[test]
fn a_joined_backward_selection_stays_backward() {
    let backward = Selection::new(Position::new(0, 6), Position::new(0, 2));
    let inside = Selection::new(Position::new(0, 3), Position::new(0, 4));

    let list = Selections::from_items(vec![backward, inside], 0);

    assert_eq!(list.primary().anchor(), Position::new(0, 6));
    assert_eq!(list.primary().head(), Position::new(0, 2));
}

#[test]
fn adding_makes_the_new_selection_primary_and_collapsing_keeps_only_it() {
    let list = Selections::single(cursor(0, 0)).with_added(cursor(4, 1));

    assert_eq!(list.len(), 2);
    assert_eq!(list.primary().head(), Position::new(4, 1));
    assert_eq!(list.collapsed(), Selections::single(cursor(4, 1)));
}

#[test]
fn an_empty_list_is_one_cursor_at_the_start() {
    assert_eq!(Selections::from_items(vec![], 0), Selections::default());
}
