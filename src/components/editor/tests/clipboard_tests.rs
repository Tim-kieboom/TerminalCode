use crate::buffer::{Buffer, Position};
use crate::clipboard::{Register, RegisterKind};
use crate::components::editor::{Editor, Motion};

fn editor(text: &str) -> Editor {
    Editor::new(Buffer::from_text(text))
}

fn head(editor: &Editor) -> Position {
    editor.selections().primary().head()
}

fn text(editor: &Editor) -> String {
    editor.buffer().text()
}

fn select_chars(editor: &mut Editor, count: usize) {
    for _ in 0..count {
        editor.select(Motion::Right).unwrap();
    }
}

#[test]
fn copy_with_a_selection_is_charwise_text() {
    let mut editor = editor("hello world");
    select_chars(&mut editor, 5);

    let register = editor.copy().unwrap();

    assert_eq!(register, Register::charwise("hello"));
}

#[test]
fn copy_with_a_multiline_selection_keeps_the_line_breaks() {
    let mut editor = editor("ab\ncd\nef");
    select_chars(&mut editor, 5);

    assert_eq!(editor.copy().unwrap(), Register::charwise("ab\ncd"));
}

#[test]
fn copy_without_a_selection_takes_the_whole_line() {
    let mut editor = editor("one\ntwo\nthree");
    editor.move_cursor(Motion::Down).unwrap();

    let register = editor.copy().unwrap();

    assert_eq!(register, Register::linewise("two\n"));
    assert_eq!(text(&editor), "one\ntwo\nthree");
}

#[test]
fn copy_of_the_last_line_still_ends_with_a_line_break() {
    let mut editor = editor("one\ntwo");
    editor.move_cursor(Motion::DocumentEnd).unwrap();

    assert_eq!(editor.copy().unwrap(), Register::linewise("two\n"));
}

#[test]
fn copying_a_crlf_line_uses_crlf() {
    let mut editor = editor("one\r\ntwo\r\n");

    assert_eq!(editor.copy().unwrap(), Register::linewise("one\r\n"));
    editor.move_cursor(Motion::Down).unwrap();
    assert_eq!(editor.copy().unwrap(), Register::linewise("two\r\n"));
}

#[test]
fn copy_does_not_change_the_buffer_or_cursor() {
    let mut editor = editor("hello");
    select_chars(&mut editor, 2);
    let before = *editor.selections().primary();

    editor.copy().unwrap();

    assert_eq!(*editor.selections().primary(), before);
    assert!(!editor.buffer().is_dirty());
}

#[test]
fn cut_a_selection_removes_it_and_returns_it() {
    let mut editor = editor("hello world");
    select_chars(&mut editor, 6);

    let register = editor.cut().unwrap();

    assert_eq!(register, Register::charwise("hello "));
    assert_eq!(text(&editor), "world");
    assert_eq!(head(&editor), Position::new(0, 0));
}

#[test]
fn cut_without_a_selection_removes_the_whole_line() {
    let mut editor = editor("one\ntwo\nthree");
    editor.move_cursor(Motion::Down).unwrap();

    let register = editor.cut().unwrap();

    assert_eq!(register, Register::linewise("two\n"));
    assert_eq!(text(&editor), "one\nthree");
    assert_eq!(head(&editor), Position::new(1, 0));
}

#[test]
fn cut_of_the_last_line_removes_the_break_before_it() {
    let mut editor = editor("one\ntwo");
    editor.move_cursor(Motion::DocumentEnd).unwrap();

    editor.cut().unwrap();

    assert_eq!(text(&editor), "one");
    assert_eq!(head(&editor), Position::new(0, 3));
}

#[test]
fn cut_of_the_only_line_empties_the_buffer() {
    let mut editor = editor("only");

    editor.cut().unwrap();

    assert_eq!(text(&editor), "");
}

#[test]
fn cut_can_be_undone_in_one_step() {
    let mut editor = editor("one\ntwo\nthree");
    editor.move_cursor(Motion::Down).unwrap();
    editor.cut().unwrap();

    editor.undo().unwrap();

    assert_eq!(text(&editor), "one\ntwo\nthree");
    assert_eq!(head(&editor), Position::new(1, 0));
}

#[test]
fn paste_charwise_inserts_at_the_cursor_and_lands_after_it() {
    let mut editor = editor("held");
    editor.move_cursor(Motion::Right).unwrap();
    editor.move_cursor(Motion::Right).unwrap();

    editor.paste(&Register::charwise("llo wor")).unwrap();

    assert_eq!(text(&editor), "hello world");
    assert_eq!(head(&editor), Position::new(0, 9));
}

#[test]
fn paste_charwise_replaces_the_selection() {
    let mut editor = editor("hello world");
    select_chars(&mut editor, 5);

    editor.paste(&Register::charwise("bye")).unwrap();

    assert_eq!(text(&editor), "bye world");
}

#[test]
fn paste_linewise_goes_above_the_current_line_and_keeps_the_cursor_on_its_character() {
    let mut editor = editor("one\ntwo\nthree");
    editor.move_cursor(Motion::Down).unwrap();
    editor.move_cursor(Motion::Right).unwrap();

    editor.paste(&Register::linewise("NEW\n")).unwrap();

    assert_eq!(text(&editor), "one\nNEW\ntwo\nthree");
    assert_eq!(head(&editor), Position::new(2, 1));
}

#[test]
fn paste_several_lines_above_moves_the_cursor_down_by_all_of_them() {
    let mut editor = editor("a\nb");

    editor.paste(&Register::linewise("x\ny\n")).unwrap();

    assert_eq!(text(&editor), "x\ny\na\nb");
    assert_eq!(head(&editor), Position::new(2, 0));
}

#[test]
fn paste_linewise_without_a_trailing_break_still_makes_whole_lines() {
    let mut editor = editor("a");

    editor.paste(&Register::linewise("x")).unwrap();

    assert_eq!(text(&editor), "x\na");
}

#[test]
fn paste_linewise_over_a_selection_replaces_just_the_selection() {
    let mut editor = editor("hello world");
    select_chars(&mut editor, 5);

    editor.paste(&Register::linewise("bye\n")).unwrap();

    assert_eq!(text(&editor), "bye world");
}

#[test]
fn copy_then_paste_of_a_line_duplicates_it_above() {
    let mut editor = editor("one\ntwo");
    editor.move_cursor(Motion::Down).unwrap();
    let register = editor.copy().unwrap();
    assert_eq!(register.kind(), RegisterKind::Linewise);

    editor.paste(&register).unwrap();

    assert_eq!(text(&editor), "one\ntwo\ntwo");
}

#[test]
fn pasted_line_breaks_are_converted_to_the_buffers_style() {
    let mut lf = editor("ab");
    let mut crlf = editor("a\r\nb");

    lf.paste(&Register::charwise("x\r\ny\rz")).unwrap();
    crlf.paste(&Register::charwise("x\ny")).unwrap();

    assert_eq!(text(&lf), "x\ny\nzab");
    assert_eq!(text(&crlf), "x\r\nya\r\nb");
}

#[test]
fn paste_is_one_undo_step_and_does_not_merge_with_typing() {
    let mut editor = editor("");
    editor.insert_text("a").unwrap();
    editor.paste(&Register::charwise("BIG\nPASTE")).unwrap();
    editor.insert_text("b").unwrap();

    editor.undo().unwrap();
    assert_eq!(text(&editor), "aBIG\nPASTE");

    editor.undo().unwrap();
    assert_eq!(text(&editor), "a");
}

#[test]
fn pasting_empty_text_changes_nothing() {
    let mut editor = editor("abc");

    editor.paste(&Register::charwise("")).unwrap();

    assert_eq!(text(&editor), "abc");
}
