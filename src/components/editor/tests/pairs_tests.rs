use std::path::PathBuf;

use crate::buffer::{Buffer, Position};
use crate::components::editor::pairs::{Typed, decide};
use crate::components::editor::{AutoClose, Editor, Motion, ViewState};

/// An editor on `text` in a file called `name`, with the cursor at the end.
fn editor_in(name: &str, text: &str) -> Editor {
    let mut buffer = Buffer::from_text(text);
    buffer.set_path(PathBuf::from(name));
    let mut editor = Editor::new(buffer);
    editor.move_cursor(Motion::DocumentEnd).unwrap();
    editor
}

fn text(editor: &Editor) -> String {
    editor.buffer().text()
}

fn head(editor: &Editor) -> Position {
    editor.selections().primary().head()
}

fn type_str(editor: &mut Editor, text: &str) {
    for c in text.chars() {
        editor.type_text(&c.to_string()).unwrap();
    }
}

fn selected(editor: &Editor) -> String {
    let selection = editor.selections().primary();
    let (start, end) = (selection.start(), selection.end());
    let from = editor.buffer().position_to_byte(start).unwrap();
    let to = editor.buffer().position_to_byte(end).unwrap();
    editor.buffer().text_in(from..to).unwrap()
}

#[test]
fn an_opener_writes_its_closer_with_the_cursor_between() {
    for (open, close) in [('(', ')'), ('[', ']'), ('{', '}'), ('"', '"')] {
        let mut editor = editor_in("a.rs", "");
        type_str(&mut editor, &open.to_string());
        assert_eq!(text(&editor), format!("{open}{close}"));
        assert_eq!(head(&editor), Position::new(0, 1));
    }
}

#[test]
fn the_closer_typed_next_steps_over_the_one_that_is_there() {
    let mut editor = editor_in("a.rs", "");
    type_str(&mut editor, "f(a)");
    assert_eq!(text(&editor), "f(a)");
    assert_eq!(head(&editor), Position::new(0, 4));
}

#[test]
fn stepping_over_does_not_depend_on_who_wrote_the_closer() {
    let mut editor = editor_in("a.rs", "(a)");
    editor.move_cursor(Motion::DocumentStart).unwrap();
    editor.move_cursor(Motion::Right).unwrap();
    editor.move_cursor(Motion::Right).unwrap();
    type_str(&mut editor, ")");
    assert_eq!(text(&editor), "(a)");
    assert_eq!(head(&editor), Position::new(0, 3));
}

#[test]
fn an_opener_before_a_word_is_alone() {
    let mut editor = editor_in("a.rs", "foo");
    editor.move_cursor(Motion::DocumentStart).unwrap();
    type_str(&mut editor, "(");
    assert_eq!(text(&editor), "(foo");
}

#[test]
fn an_opener_before_a_closer_or_a_space_pairs() {
    let mut editor = editor_in("a.rs", "f()");
    editor.move_cursor(Motion::Left).unwrap();
    type_str(&mut editor, "[");
    assert_eq!(text(&editor), "f([])");
    let mut editor = editor_in("a.rs", " x");
    editor.move_cursor(Motion::DocumentStart).unwrap();
    type_str(&mut editor, "{");
    assert_eq!(text(&editor), "{} x");
}

#[test]
fn a_quote_after_a_word_is_not_paired() {
    let mut editor = editor_in("a.js", "");
    type_str(&mut editor, "don'");
    assert_eq!(text(&editor), "don'");
}

#[test]
fn a_quote_in_front_of_a_string_is_alone_but_the_end_of_a_string_is_stepped_over() {
    let mut editor = editor_in("a.rs", "x = \"foo\"");
    editor.move_cursor(Motion::DocumentStart).unwrap();
    for _ in 0..4 {
        editor.move_cursor(Motion::Right).unwrap();
    }
    type_str(&mut editor, "\"");
    assert_eq!(text(&editor), "x = \"\"foo\"");

    let mut editor = editor_in("a.rs", "");
    type_str(&mut editor, "\"foo\"");
    assert_eq!(text(&editor), "\"foo\"");
    assert_eq!(head(&editor), Position::new(0, 5));
}

#[test]
fn rust_has_no_single_quote_pair_but_javascript_has() {
    let mut editor = editor_in("a.rs", "");
    type_str(&mut editor, "'");
    assert_eq!(text(&editor), "'");
    let mut editor = editor_in("a.js", "");
    type_str(&mut editor, "'");
    assert_eq!(text(&editor), "''");
}

#[test]
fn json_has_no_single_quote_pair_and_markdown_has_the_backtick() {
    let mut editor = editor_in("a.json", "");
    type_str(&mut editor, "'");
    assert_eq!(text(&editor), "'");
    let mut editor = editor_in("a.md", "");
    type_str(&mut editor, "`");
    assert_eq!(text(&editor), "``");
}

#[test]
fn backspace_between_an_empty_pair_takes_both() {
    let mut editor = editor_in("a.rs", "");
    type_str(&mut editor, "(");
    editor.delete_backward().unwrap();
    assert_eq!(text(&editor), "");
    type_str(&mut editor, "(a");
    editor.delete_backward().unwrap();
    assert_eq!(text(&editor), "()");
    editor.delete_backward().unwrap();
    assert_eq!(text(&editor), "");
}

#[test]
fn an_opener_over_a_selection_wraps_it_and_keeps_it_selected() {
    let mut editor = editor_in("a.rs", "foo bar");
    editor.select_range(Position::new(0, 4), Position::new(0, 7));
    type_str(&mut editor, "(");
    assert_eq!(text(&editor), "foo (bar)");
    assert_eq!(selected(&editor), "bar");
    type_str(&mut editor, "\"");
    assert_eq!(text(&editor), "foo (\"bar\")");
    assert_eq!(selected(&editor), "bar");
}

#[test]
fn a_closer_over_a_selection_replaces_it() {
    let mut editor = editor_in("a.rs", "foo bar");
    editor.select_range(Position::new(0, 4), Position::new(0, 7));
    type_str(&mut editor, ")");
    assert_eq!(text(&editor), "foo )");
}

#[test]
fn typing_a_pair_is_one_undo_step_with_the_typed_character() {
    let mut editor = editor_in("a.rs", "x ");
    type_str(&mut editor, "(");
    editor.undo().unwrap();
    assert_eq!(text(&editor), "x ");
}

#[test]
fn every_cursor_pairs_and_steps_over_by_itself() {
    let mut editor = editor_in("a.rs", "a\nb");
    editor.add_cursor(Position::new(0, 1));
    type_str(&mut editor, "(");
    assert_eq!(text(&editor), "a()\nb()");
    type_str(&mut editor, "x");
    type_str(&mut editor, ")");
    assert_eq!(text(&editor), "a(x)\nb(x)");
}

#[test]
fn one_cursor_that_steps_over_while_another_pairs_keeps_both_right() {
    let mut editor = editor_in("a.rs", "()\n");
    editor.move_cursor(Motion::DocumentStart).unwrap();
    editor.move_cursor(Motion::Right).unwrap();
    editor.add_cursor(Position::new(1, 0));
    type_str(&mut editor, ")");
    assert_eq!(text(&editor), "()\n)");
}

#[test]
fn the_setting_off_types_only_what_is_typed() {
    let mut buffer = Buffer::from_text("");
    buffer.set_path(PathBuf::from("a.rs"));
    let mut view = ViewState::default();
    view.set_auto_close(AutoClose::Off);
    let mut editor = Editor::from_parts(view, buffer);
    type_str(&mut editor, "(");
    assert_eq!(text(&editor), "(");
    editor.delete_backward().unwrap();
    type_str(&mut editor, "()");
    editor.move_cursor(Motion::Left).unwrap();
    editor.delete_backward().unwrap();
    assert_eq!(text(&editor), ")");
}

#[test]
fn text_with_more_than_one_character_is_not_paired() {
    let mut editor = editor_in("a.rs", "");
    editor.type_text("(a").unwrap();
    assert_eq!(text(&editor), "(a");
}

#[test]
fn a_closer_on_an_indent_only_line_still_takes_a_level_away() {
    let mut editor = editor_in("a.rs", "fn f() {\n    ");
    type_str(&mut editor, "}");
    assert_eq!(text(&editor), "fn f() {\n}");
}

#[test]
fn enter_between_a_typed_pair_opens_a_line_for_the_cursor() {
    let mut editor = editor_in("a.rs", "fn f() ");
    type_str(&mut editor, "{");
    editor.insert_newline().unwrap();
    assert_eq!(text(&editor), "fn f() {\n    \n}");
    assert_eq!(head(&editor), Position::new(1, 4));
}

#[test]
fn decide_covers_the_table() {
    let rust: &[(char, char)] = &[('(', ')'), ('"', '"')];
    assert_eq!(decide(rust, 'a', None, None, false), Typed::Plain);
    assert_eq!(decide(rust, '(', Some('f'), None, false), Typed::Pair(')'));
    assert_eq!(decide(rust, '(', None, Some('x'), false), Typed::Plain);
    assert_eq!(decide(rust, ')', None, Some(')'), false), Typed::StepOver);
    assert_eq!(decide(rust, ')', None, Some(']'), false), Typed::Plain);
    assert_eq!(decide(rust, '"', Some(' '), None, false), Typed::Pair('"'));
    assert_eq!(decide(rust, '"', Some('a'), None, false), Typed::Plain);
    assert_eq!(
        decide(rust, '"', Some('a'), Some('"'), false),
        Typed::StepOver
    );
    assert_eq!(
        decide(rust, '"', Some('"'), Some('"'), false),
        Typed::StepOver
    );
    assert_eq!(decide(rust, '"', Some(' '), Some('"'), false), Typed::Plain);
    assert_eq!(decide(rust, '(', None, None, true), Typed::Wrap(')'));
    assert_eq!(decide(rust, ')', None, None, true), Typed::Plain);
}
