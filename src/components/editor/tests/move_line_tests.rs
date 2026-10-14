use crate::buffer::{Buffer, Position};
use crate::components::editor::Editor;

fn editor(text: &str) -> Editor {
    Editor::new(Buffer::from_text(text))
}

fn text(editor: &Editor) -> String {
    editor.buffer().text()
}

fn head(editor: &Editor) -> Position {
    editor.selections().primary().head()
}

#[test]
fn a_line_moves_down_with_its_cursor() {
    let mut editor = editor("one\ntwo\nthree");
    editor.go_to(Position::new(0, 2));

    editor.move_lines(1).unwrap();

    assert_eq!(text(&editor), "two\none\nthree");
    assert_eq!(head(&editor), Position::new(1, 2));
}

#[test]
fn a_line_moves_up_with_its_cursor() {
    let mut editor = editor("one\ntwo\nthree");
    editor.go_to(Position::new(2, 4));

    editor.move_lines(-1).unwrap();

    assert_eq!(text(&editor), "one\nthree\ntwo");
    assert_eq!(head(&editor), Position::new(1, 4));
}

#[test]
fn nothing_moves_past_the_first_or_last_line() {
    let mut editor = editor("one\ntwo");

    editor.move_lines(-1).unwrap();
    assert_eq!(text(&editor), "one\ntwo");

    editor.go_to(Position::new(1, 0));
    editor.move_lines(1).unwrap();
    assert_eq!(text(&editor), "one\ntwo");
}

#[test]
fn selected_lines_move_together_and_stay_selected() {
    let mut editor = editor("a\nb\nc\nd");
    editor.select_range(Position::new(1, 0), Position::new(2, 1));

    editor.move_lines(1).unwrap();

    assert_eq!(text(&editor), "a\nd\nb\nc");
    let selection = editor.selections().primary();
    assert_eq!(selection.start(), Position::new(2, 0));
    assert_eq!(selection.end(), Position::new(3, 1));
}

#[test]
fn a_selection_ending_at_the_start_of_a_line_leaves_that_line_alone() {
    let mut editor = editor("a\nb\nc\nd");
    editor.select_range(Position::new(0, 0), Position::new(2, 0));

    editor.move_lines(1).unwrap();

    assert_eq!(text(&editor), "c\na\nb\nd");
}

#[test]
fn the_last_line_without_a_line_break_moves_up_and_keeps_none() {
    let mut editor = editor("a\nb");
    editor.go_to(Position::new(1, 0));

    editor.move_lines(-1).unwrap();

    assert_eq!(text(&editor), "b\na");
}

#[test]
fn crlf_endings_are_kept() {
    let mut editor = editor("a\r\nb\r\nc");
    editor.go_to(Position::new(0, 0));

    editor.move_lines(1).unwrap();

    assert_eq!(text(&editor), "b\r\na\r\nc");
}

#[test]
fn a_move_is_one_undo_step() {
    let mut editor = editor("a\nb\nc");
    editor.move_lines(1).unwrap();
    editor.move_lines(1).unwrap();
    assert_eq!(text(&editor), "b\nc\na");

    editor.undo().unwrap();

    assert_eq!(text(&editor), "b\na\nc");
}

#[test]
fn several_cursors_leave_one_after_a_move() {
    let mut editor = editor("a\nb\nc");
    editor.add_cursor_vertically(1).unwrap();

    editor.move_lines(1).unwrap();

    assert!(!editor.has_many_cursors());
    assert_eq!(text(&editor), "a\nc\nb");
}
