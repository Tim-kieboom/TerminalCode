use crate::buffer::{Buffer, Position};
use crate::components::editor::{Editor, Motion};

fn editor(text: &str) -> Editor {
    Editor::new(Buffer::from_text(text))
}

fn text(editor: &Editor) -> String {
    editor.buffer().text()
}

fn heads(editor: &Editor) -> Vec<Position> {
    editor.selections().iter().map(|s| s.head()).collect()
}

/// An editor with a cursor at the start of each of the first `count` lines.
fn cursors_down(text: &str, count: usize) -> Editor {
    let mut editor = editor(text);
    for _ in 1..count {
        editor.add_cursor_vertically(1).unwrap();
    }
    editor
}

#[test]
fn a_cursor_is_added_below_in_the_wanted_column_and_becomes_primary() {
    let mut editor = editor("abcdef\nab\nabcdef");
    editor.move_cursor(Motion::LineEnd).unwrap();

    editor.add_cursor_vertically(1).unwrap();
    editor.add_cursor_vertically(1).unwrap();

    assert_eq!(
        heads(&editor),
        vec![
            Position::new(0, 6),
            Position::new(1, 2),
            Position::new(2, 6)
        ]
    );
    assert_eq!(editor.selections().primary().head(), Position::new(2, 6));
}

#[test]
fn no_cursor_is_added_past_the_first_or_last_line() {
    let mut editor = editor("a\nb");

    editor.add_cursor_vertically(-1).unwrap();
    assert!(!editor.has_many_cursors());

    editor.add_cursor_vertically(1).unwrap();
    editor.add_cursor_vertically(1).unwrap();
    assert_eq!(editor.selections().len(), 2);
}

#[test]
fn typing_inserts_at_every_cursor_as_one_undo_step() {
    let mut editor = cursors_down("a\nb\nc", 3);

    editor.insert_text("x").unwrap();
    editor.insert_text("y").unwrap();

    assert_eq!(text(&editor), "xya\nxyb\nxyc");
    assert_eq!(
        heads(&editor),
        vec![
            Position::new(0, 2),
            Position::new(1, 2),
            Position::new(2, 2)
        ]
    );

    editor.undo().unwrap();
    assert_eq!(
        text(&editor),
        "a\nb\nc",
        "one undo takes back the whole burst"
    );
    assert_eq!(editor.selections().len(), 3, "and the cursors come back");

    editor.redo().unwrap();
    assert_eq!(text(&editor), "xya\nxyb\nxyc");
}

#[test]
fn typing_twice_on_one_line_keeps_each_cursor_after_its_text() {
    let mut editor = editor("ab");
    editor.move_cursor(Motion::LineEnd).unwrap();
    editor.add_cursor(Position::new(0, 0));

    editor.insert_text("-").unwrap();

    assert_eq!(text(&editor), "-ab-");
    assert_eq!(
        heads(&editor),
        vec![Position::new(0, 1), Position::new(0, 4)]
    );
}

#[test]
fn typing_over_several_selections_replaces_each() {
    let mut editor = editor("one two three");
    editor.select_range(Position::new(0, 0), Position::new(0, 3));
    editor.add_cursor(Position::new(0, 8));
    editor.select(Motion::Right).unwrap();

    editor.insert_text("#").unwrap();

    assert_eq!(text(&editor), "#two #hree");
}

#[test]
fn backspace_and_delete_work_on_every_cursor() {
    let mut editor = editor("abc\nabc\nabc");
    editor.move_cursor(Motion::Right).unwrap();
    editor.move_cursor(Motion::Right).unwrap();
    editor.add_cursor_vertically(1).unwrap();
    editor.add_cursor_vertically(1).unwrap();

    editor.delete_backward().unwrap();
    assert_eq!(text(&editor), "ac\nac\nac");

    editor.delete_forward().unwrap();
    assert_eq!(text(&editor), "a\na\na");
}

#[test]
fn backspace_at_the_start_of_a_line_joins_it_even_with_a_cursor_on_the_line_above() {
    let mut editor = editor("ab\ncd\nef");
    editor.move_cursor(Motion::LineEnd).unwrap();
    editor.add_cursor(Position::new(1, 0));

    editor.delete_backward().unwrap();

    assert_eq!(text(&editor), "acd\nef");
    assert_eq!(
        heads(&editor),
        vec![Position::new(0, 1)],
        "both land in one place"
    );
}

#[test]
fn deletes_that_run_into_each_other_are_joined() {
    let mut editor = editor("ab");
    editor.move_cursor(Motion::Right).unwrap();
    editor.add_cursor(Position::new(0, 2));
    // Both forward word deletes want the "b": from 1 to the end and from 2 to the end.
    editor.delete_word_backward().unwrap();

    assert_eq!(text(&editor), "");
    assert!(!editor.has_many_cursors());
}

#[test]
fn a_line_break_at_every_cursor_keeps_each_lines_indentation() {
    let mut editor = editor("  a\nb\n    c");
    editor.go_to(Position::new(0, 3));
    editor.add_cursor(Position::new(1, 1));
    editor.add_cursor(Position::new(2, 5));

    editor.insert_newline().unwrap();

    assert_eq!(text(&editor), "  a\n  \nb\n\n    c\n    ");
    assert_eq!(editor.selections().len(), 3);
}

#[test]
fn motions_move_every_cursor_and_cursors_that_meet_become_one() {
    let mut editor = cursors_down("abc\nabc\nabc", 3);

    editor.move_cursor(Motion::LineEnd).unwrap();
    assert_eq!(
        heads(&editor),
        vec![
            Position::new(0, 3),
            Position::new(1, 3),
            Position::new(2, 3)
        ]
    );

    editor.move_cursor(Motion::Down).unwrap();
    assert_eq!(
        heads(&editor),
        vec![Position::new(1, 3), Position::new(2, 3)],
        "the cursor on the last line stays there and the one above joins it"
    );
}

#[test]
fn selecting_by_motion_extends_every_selection() {
    let mut editor = cursors_down("abc\nabc", 2);

    editor.select(Motion::Right).unwrap();
    editor.select(Motion::Right).unwrap();
    editor.insert_text("").unwrap();

    assert_eq!(text(&editor), "c\nc");
}

#[test]
fn something_that_knows_one_cursor_only_leaves_one() {
    let mut editor = cursors_down("a\nb", 2);

    editor.indent().unwrap();

    assert!(!editor.has_many_cursors());
}

#[test]
fn collapsing_keeps_the_primary_cursor() {
    let mut editor = cursors_down("a\nb\nc", 3);

    editor.collapse_cursors();

    assert_eq!(heads(&editor), vec![Position::new(2, 0)]);
}
