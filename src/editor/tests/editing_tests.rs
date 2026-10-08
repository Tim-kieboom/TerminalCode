use crate::buffer::{Buffer, Position};
use crate::editor::{Editor, Motion};

fn editor(text: &str) -> Editor {
    Editor::new(Buffer::from_text(text))
}

fn head(editor: &Editor) -> Position {
    editor.selections().primary().head()
}

fn text(editor: &Editor) -> String {
    editor.buffer().text()
}

fn goto_end(editor: &mut Editor) {
    editor.move_cursor(Motion::DocumentEnd).unwrap();
}

#[test]
fn typing_inserts_at_the_cursor_and_moves_it_after_the_text() {
    let mut editor = editor("held");
    editor.move_cursor(Motion::Right).unwrap();
    editor.move_cursor(Motion::Right).unwrap();

    editor.insert_text("llo wor").unwrap();

    assert_eq!(text(&editor), "hello world");
    assert_eq!(head(&editor), Position::new(0, 9));
}

#[test]
fn typing_over_a_selection_replaces_it() {
    let mut editor = editor("hello world");
    for _ in 0..5 {
        editor.select(Motion::Right).unwrap();
    }

    editor.insert_text("J").unwrap();

    assert_eq!(text(&editor), "J world");
    assert_eq!(head(&editor), Position::new(0, 1));
    assert!(editor.selections().primary().is_empty());
}

#[test]
fn newline_uses_the_files_line_ending() {
    let mut lf = editor("ab");
    let mut crlf = editor("a\r\nb");
    lf.move_cursor(Motion::Right).unwrap();
    crlf.move_cursor(Motion::Right).unwrap();

    lf.insert_newline().unwrap();
    crlf.insert_newline().unwrap();

    assert_eq!(text(&lf), "a\nb");
    assert_eq!(text(&crlf), "a\r\n\r\nb");
    assert_eq!(head(&lf), Position::new(1, 0));
    assert_eq!(head(&crlf), Position::new(1, 0));
}

#[test]
fn backspace_deletes_one_grapheme() {
    let mut editor = editor("e\u{301}x");
    editor.move_cursor(Motion::Right).unwrap();

    editor.delete_backward().unwrap();

    assert_eq!(text(&editor), "x");
    assert_eq!(head(&editor), Position::new(0, 0));
}

#[test]
fn backspace_at_line_start_joins_with_the_previous_line() {
    let mut editor = editor("ab\r\ncd");
    editor.move_cursor(Motion::Down).unwrap();
    editor.move_cursor(Motion::LineStart).unwrap();

    editor.delete_backward().unwrap();

    assert_eq!(text(&editor), "abcd");
    assert_eq!(head(&editor), Position::new(0, 2));
}

#[test]
fn backspace_at_the_document_start_does_nothing() {
    let mut editor = editor("ab");

    editor.delete_backward().unwrap();

    assert_eq!(text(&editor), "ab");
    assert_eq!(editor.buffer().version(), 0);
}

#[test]
fn delete_removes_the_grapheme_after_the_cursor() {
    let mut editor = editor("a\u{1F1F3}\u{1F1F1}b");
    editor.move_cursor(Motion::Right).unwrap();

    editor.delete_forward().unwrap();

    assert_eq!(text(&editor), "ab");
    assert_eq!(head(&editor), Position::new(0, 1));
}

#[test]
fn delete_at_line_end_joins_the_next_line() {
    let mut editor = editor("ab\ncd");
    editor.move_cursor(Motion::LineEnd).unwrap();

    editor.delete_forward().unwrap();

    assert_eq!(text(&editor), "abcd");
    assert_eq!(head(&editor), Position::new(0, 2));
}

#[test]
fn delete_at_the_document_end_does_nothing() {
    let mut editor = editor("ab");
    goto_end(&mut editor);

    editor.delete_forward().unwrap();

    assert_eq!(text(&editor), "ab");
}

#[test]
fn deleting_a_selection_removes_it_in_one_step() {
    let mut editor = editor("abcdef");
    for _ in 0..3 {
        editor.select(Motion::Right).unwrap();
    }

    editor.delete_backward().unwrap();

    assert_eq!(text(&editor), "def");
    editor.undo().unwrap();
    assert_eq!(text(&editor), "abcdef");
}

#[test]
fn undo_restores_text_and_cursor_and_redo_replays() {
    let mut editor = editor("ab");
    editor.move_cursor(Motion::LineEnd).unwrap();
    editor.insert_text("c").unwrap();

    editor.undo().unwrap();
    assert_eq!(text(&editor), "ab");
    assert_eq!(head(&editor), Position::new(0, 2));

    editor.redo().unwrap();
    assert_eq!(text(&editor), "abc");
    assert_eq!(head(&editor), Position::new(0, 3));
}

#[test]
fn undo_with_no_history_is_a_no_op() {
    let mut editor = editor("ab");

    editor.undo().unwrap();
    editor.redo().unwrap();

    assert_eq!(text(&editor), "ab");
}

#[test]
fn editing_makes_the_buffer_dirty() {
    let mut editor = editor("ab");
    assert!(!editor.buffer().is_dirty());

    editor.insert_text("x").unwrap();

    assert!(editor.buffer().is_dirty());
}
