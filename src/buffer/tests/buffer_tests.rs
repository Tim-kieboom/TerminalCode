use crate::buffer::edit::Edit;
use crate::buffer::position::{Point, Position};
use crate::buffer::{Buffer, BufferError};

// Regional-indicator pair (one flag), a combining sequence, and wide CJK.
const FLAG: &str = "\u{1F1F3}\u{1F1F1}";
const E_ACUTE: &str = "e\u{301}";

#[test]
fn empty_buffer_has_one_empty_line() {
    let buffer = Buffer::default();

    assert_eq!(buffer.len_lines(), 1);
    assert_eq!(buffer.len_bytes(), 0);
}

#[test]
fn insert_changes_text_and_bumps_version() {
    let mut buffer = Buffer::from_text("held");

    buffer.apply(&Edit::insert(2, "llo wor")).unwrap();

    assert_eq!(buffer.text(), "hello world");
    assert_eq!(buffer.version(), 1);
}

#[test]
fn delete_and_replace_work() {
    let mut buffer = Buffer::from_text("hello world");

    buffer.apply(&Edit::delete(5..11)).unwrap();
    buffer.apply(&Edit::new(0..1, "J")).unwrap();

    assert_eq!(buffer.text(), "Jello");
    assert_eq!(buffer.version(), 2);
}

#[test]
fn applying_the_inverse_restores_the_text() {
    let original = format!("a{FLAG}\nこんにちは\n{E_ACUTE}x");
    let edits = [
        Edit::insert(1, "XYZ"),
        Edit::delete(0..9),
        Edit::new(10..16, "line1\nline2\n"),
    ];

    for edit in edits {
        let mut buffer = Buffer::from_text(&original);
        let applied = buffer.apply(&edit).unwrap();

        buffer.apply(&applied.inverse).unwrap();

        assert_eq!(buffer.text(), original, "edit {edit:?}");
    }
}

#[test]
fn apply_reports_tree_sitter_positions() {
    let mut buffer = Buffer::from_text("ab\ncd\nef");

    let info = buffer.apply(&Edit::new(1..4, "X\nY\nZ")).unwrap().info;

    assert_eq!(info.start_byte, 1);
    assert_eq!(info.old_end_byte, 4);
    assert_eq!(info.new_end_byte, 6);
    assert_eq!(info.start_point, Point { row: 0, column: 1 });
    assert_eq!(info.old_end_point, Point { row: 1, column: 1 });
    assert_eq!(info.new_end_point, Point { row: 2, column: 1 });
}

#[test]
fn out_of_bounds_edit_is_rejected_and_buffer_unchanged() {
    let mut buffer = Buffer::from_text("abc");

    let result = buffer.apply(&Edit::delete(1..9));

    assert_eq!(
        result.unwrap_err(),
        BufferError::ByteOutOfBounds { offset: 9, len: 3 }
    );
    assert_eq!(buffer.text(), "abc");
    assert_eq!(buffer.version(), 0);
}

#[test]
fn edit_inside_a_multibyte_char_is_rejected() {
    let mut buffer = Buffer::from_text("こ");

    let result = buffer.apply(&Edit::insert(1, "x"));

    assert_eq!(result.unwrap_err(), BufferError::NotCharBoundary(1));
}

#[test]
fn inverted_range_is_rejected() {
    let mut buffer = Buffer::from_text("abc");

    let (start, end) = (2, 1);

    let result = buffer.apply(&Edit::new(start..end, ""));

    assert_eq!(
        result.unwrap_err(),
        BufferError::InvertedRange { start: 2, end: 1 }
    );
}

#[test]
fn position_to_byte_counts_graphemes_not_chars() {
    let buffer = Buffer::from_text(&format!("a{FLAG}{E_ACUTE}b"));

    let after_flag = buffer.position_to_byte(Position::new(0, 2)).unwrap();
    let before_b = buffer.position_to_byte(Position::new(0, 3)).unwrap();
    let end = buffer.position_to_byte(Position::new(0, 4)).unwrap();

    assert_eq!(after_flag, 1 + FLAG.len());
    assert_eq!(before_b, 1 + FLAG.len() + E_ACUTE.len());
    assert_eq!(end, buffer.len_bytes());
}

#[test]
fn position_to_byte_excludes_line_breaks() {
    let buffer = Buffer::from_text("ab\r\ncd\n");

    assert_eq!(buffer.position_to_byte(Position::new(0, 2)), Ok(2));
    assert_eq!(buffer.position_to_byte(Position::new(1, 0)), Ok(4));
    assert_eq!(buffer.position_to_byte(Position::new(1, 2)), Ok(6));
    assert_eq!(buffer.position_to_byte(Position::new(2, 0)), Ok(7));
}

#[test]
fn position_past_line_end_is_rejected() {
    let buffer = Buffer::from_text("ab\ncd");

    assert_eq!(
        buffer.position_to_byte(Position::new(0, 3)),
        Err(BufferError::ColumnOutOfBounds {
            line: 0,
            column: 3,
            columns: 2
        })
    );
}

#[test]
fn position_on_missing_line_is_rejected() {
    let buffer = Buffer::from_text("ab");

    assert_eq!(
        buffer.position_to_byte(Position::new(1, 0)),
        Err(BufferError::LineOutOfBounds { line: 1, lines: 1 })
    );
}

#[test]
fn byte_to_position_round_trips_on_grapheme_boundaries() {
    let buffer = Buffer::from_text(&format!("a{FLAG}{E_ACUTE}\nこb"));
    let positions = [
        Position::new(0, 0),
        Position::new(0, 1),
        Position::new(0, 2),
        Position::new(0, 3),
        Position::new(1, 0),
        Position::new(1, 1),
        Position::new(1, 2),
    ];

    for position in positions {
        let byte = buffer.position_to_byte(position).unwrap();

        assert_eq!(buffer.byte_to_position(byte), Ok(position));
    }
}

#[test]
fn byte_inside_a_grapheme_maps_to_its_start() {
    let buffer = Buffer::from_text(&format!("a{E_ACUTE}"));

    // Byte 2 is between `e` and the combining accent.
    assert_eq!(buffer.byte_to_position(2), Ok(Position::new(0, 1)));
}

#[test]
fn byte_inside_crlf_maps_to_line_end() {
    let buffer = Buffer::from_text("ab\r\ncd");

    assert_eq!(buffer.byte_to_position(3), Ok(Position::new(0, 2)));
}

#[test]
fn applied_edits_are_logged_in_order_and_drained_once() {
    let mut buffer = Buffer::from_text("abc");
    buffer.apply(&Edit::insert(1, "XY")).unwrap();
    buffer.apply(&Edit::delete(0..1)).unwrap();

    let log = buffer.take_edit_log();

    assert_eq!(log.len(), 2);
    assert_eq!(
        (log[0].start_byte, log[0].old_end_byte, log[0].new_end_byte),
        (1, 1, 3)
    );
    assert_eq!(
        (log[1].start_byte, log[1].old_end_byte, log[1].new_end_byte),
        (0, 1, 0)
    );
    assert!(buffer.take_edit_log().is_empty());
}

#[test]
fn failed_edits_are_not_logged() {
    let mut buffer = Buffer::from_text("abc");

    assert!(buffer.apply(&Edit::delete(1..9)).is_err());

    assert!(buffer.take_edit_log().is_empty());
}

#[test]
fn undo_and_redo_are_logged_too() {
    use crate::buffer::selection::Selections;
    let mut buffer = Buffer::from_text("abc");
    let mut transaction = buffer.begin_transaction(Selections::default());
    transaction.apply(&Edit::insert(3, "d")).unwrap();
    transaction.commit(Selections::default());
    buffer.take_edit_log();

    buffer.undo().unwrap();
    buffer.redo().unwrap();

    assert_eq!(buffer.take_edit_log().len(), 2);
}

#[test]
fn offsets_before_an_edit_do_not_move() {
    let mut buffer = Buffer::from_text("hello world");
    let info = buffer.apply(&Edit::new(6..11, "there")).unwrap().info;

    assert_eq!(info.remap_byte(0), 0);
    assert_eq!(info.remap_byte(6), 6);
}

#[test]
fn offsets_after_an_edit_shift_by_its_size_change() {
    let mut buffer = Buffer::from_text("abcdef");
    let grow = buffer.apply(&Edit::insert(2, "XXX")).unwrap().info;
    let shrink = buffer.apply(&Edit::delete(0..2)).unwrap().info;

    assert_eq!(grow.remap_byte(4), 7);
    assert_eq!(shrink.remap_byte(7), 5);
}

#[test]
fn an_offset_at_an_insertion_point_stays_before_the_new_text() {
    let mut buffer = Buffer::from_text("ab");
    let info = buffer.apply(&Edit::insert(1, "XYZ")).unwrap().info;

    assert_eq!(info.remap_byte(1), 1);
    assert_eq!(info.remap_byte(2), 5);
}

#[test]
fn offsets_inside_a_replaced_range_collapse_to_its_start() {
    let mut buffer = Buffer::from_text("abcdef");
    let info = buffer.apply(&Edit::new(1..5, "Z")).unwrap().info;

    assert_eq!(info.remap_byte(3), 1);
    assert_eq!(info.remap_byte(5), 2);
}
