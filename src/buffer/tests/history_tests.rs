use crate::buffer::Buffer;
use crate::buffer::edit::Edit;
use crate::buffer::position::Position;
use crate::buffer::selection::{Selection, Selections};

fn cursor(line: usize, column: usize) -> Selections {
    Selections::single(Selection::cursor(Position::new(line, column)))
}

/// Applies each edit in one committed transaction.
fn commit(buffer: &mut Buffer, before: Selections, edits: &[Edit], after: Selections) {
    let mut transaction = buffer.begin_transaction(before);
    for edit in edits {
        transaction.apply(edit).unwrap();
    }
    transaction.commit(after);
}

#[test]
fn undo_with_nothing_to_undo_returns_none() {
    let mut buffer = Buffer::from_text("abc");

    assert_eq!(buffer.undo().unwrap(), None);
    assert_eq!(buffer.redo().unwrap(), None);
}

#[test]
fn undo_restores_text_and_returns_selections_from_before() {
    let mut buffer = Buffer::from_text("abc");
    commit(
        &mut buffer,
        cursor(0, 3),
        &[Edit::insert(3, "d")],
        cursor(0, 4),
    );

    let restored = buffer.undo().unwrap();

    assert_eq!(buffer.text(), "abc");
    assert_eq!(restored, Some(cursor(0, 3)));
}

#[test]
fn redo_replays_the_step_and_returns_selections_from_after() {
    let mut buffer = Buffer::from_text("abc");
    commit(
        &mut buffer,
        cursor(0, 3),
        &[Edit::insert(3, "d")],
        cursor(0, 4),
    );
    buffer.undo().unwrap();

    let restored = buffer.redo().unwrap();

    assert_eq!(buffer.text(), "abcd");
    assert_eq!(restored, Some(cursor(0, 4)));
}

#[test]
fn all_edits_of_a_transaction_undo_together() {
    let mut buffer = Buffer::from_text("hello world");
    let edits = [
        Edit::delete(0..5),
        Edit::insert(0, "goodbye"),
        Edit::insert(7, "!"),
    ];
    commit(&mut buffer, cursor(0, 0), &edits, cursor(0, 8));
    assert_eq!(buffer.text(), "goodbye! world");

    buffer.undo().unwrap();

    assert_eq!(buffer.text(), "hello world");
    assert_eq!(buffer.undo().unwrap(), None);
}

#[test]
fn separate_transactions_undo_one_at_a_time() {
    let mut buffer = Buffer::from_text("");
    commit(
        &mut buffer,
        cursor(0, 0),
        &[Edit::insert(0, "a")],
        cursor(0, 1),
    );
    commit(
        &mut buffer,
        cursor(0, 1),
        &[Edit::insert(1, "b")],
        cursor(0, 2),
    );

    buffer.undo().unwrap();
    assert_eq!(buffer.text(), "a");

    buffer.undo().unwrap();
    assert_eq!(buffer.text(), "");
}

#[test]
fn a_new_change_clears_the_redo_stack() {
    let mut buffer = Buffer::from_text("a");
    commit(
        &mut buffer,
        cursor(0, 1),
        &[Edit::insert(1, "b")],
        cursor(0, 2),
    );
    buffer.undo().unwrap();

    commit(
        &mut buffer,
        cursor(0, 1),
        &[Edit::insert(1, "c")],
        cursor(0, 2),
    );

    assert_eq!(buffer.redo().unwrap(), None);
    assert_eq!(buffer.text(), "ac");
}

#[test]
fn empty_transaction_leaves_no_undo_step() {
    let mut buffer = Buffer::from_text("abc");

    commit(&mut buffer, cursor(0, 0), &[], cursor(0, 0));

    assert_eq!(buffer.undo().unwrap(), None);
}

#[test]
fn dropping_a_transaction_without_commit_rolls_it_back() {
    let mut buffer = Buffer::from_text("abc");
    {
        let mut transaction = buffer.begin_transaction(cursor(0, 0));
        transaction.apply(&Edit::insert(0, "xyz")).unwrap();
        transaction.apply(&Edit::delete(0..1)).unwrap();
    }

    assert_eq!(buffer.text(), "abc");
    assert_eq!(buffer.undo().unwrap(), None);
}

#[test]
fn failed_edit_inside_a_transaction_can_be_followed_by_rollback() {
    let mut buffer = Buffer::from_text("abc");
    {
        let mut transaction = buffer.begin_transaction(cursor(0, 0));
        transaction.apply(&Edit::insert(0, "x")).unwrap();
        assert!(transaction.apply(&Edit::delete(0..99)).is_err());
    }

    assert_eq!(buffer.text(), "abc");
}

#[test]
fn undo_and_redo_bump_the_version() {
    let mut buffer = Buffer::from_text("a");
    commit(
        &mut buffer,
        cursor(0, 1),
        &[Edit::insert(1, "b")],
        cursor(0, 2),
    );
    let after_edit = buffer.version();

    buffer.undo().unwrap();
    buffer.redo().unwrap();

    assert_eq!(buffer.version(), after_edit + 2);
}

#[test]
fn merged_steps_undo_and_redo_together() {
    let mut buffer = Buffer::from_text("");
    commit(
        &mut buffer,
        cursor(0, 0),
        &[Edit::insert(0, "a")],
        cursor(0, 1),
    );
    commit(
        &mut buffer,
        cursor(0, 1),
        &[Edit::insert(1, "b")],
        cursor(0, 2),
    );

    buffer.merge_last_two_steps();

    let restored = buffer.undo().unwrap();
    assert_eq!(buffer.text(), "");
    assert_eq!(restored, Some(cursor(0, 0)));
    assert_eq!(buffer.undo().unwrap(), None);

    let restored = buffer.redo().unwrap();
    assert_eq!(buffer.text(), "ab");
    assert_eq!(restored, Some(cursor(0, 2)));
}

#[test]
fn merging_with_fewer_than_two_steps_does_nothing() {
    let mut buffer = Buffer::from_text("");
    buffer.merge_last_two_steps();
    commit(
        &mut buffer,
        cursor(0, 0),
        &[Edit::insert(0, "a")],
        cursor(0, 1),
    );

    buffer.merge_last_two_steps();

    buffer.undo().unwrap();
    assert_eq!(buffer.text(), "");
}
