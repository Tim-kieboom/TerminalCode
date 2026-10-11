use std::path::PathBuf;

use crate::buffer::{Buffer, Position};
use crate::components::editor::{Editor, IndentStyle, Motion, ViewState};

/// An editor on `text` that belongs to a file called `name` (nothing is read
/// or written), with the cursor at the end of the text.
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

fn enter(editor: &mut Editor) {
    editor.insert_newline().unwrap();
}

fn type_str(editor: &mut Editor, text: &str) {
    editor.insert_text(text).unwrap();
}

/// Like [`editor_in`], with the indent style given instead of detected.
fn styled(name: &str, text: &str, style: IndentStyle) -> Editor {
    let mut buffer = Buffer::from_text(text);
    buffer.set_path(PathBuf::from(name));
    let mut editor = Editor::from_parts(ViewState::with_indent(style), buffer);
    editor.move_cursor(Motion::DocumentEnd).unwrap();
    editor
}

// ---- Enter

#[test]
fn enter_after_an_opening_brace_goes_one_level_deeper() {
    let mut editor = editor_in("a.rs", "fn main() {");
    enter(&mut editor);
    assert_eq!(text(&editor), "fn main() {\n    ");
    assert_eq!(head(&editor), Position::new(1, 4));
}

#[test]
fn enter_adds_a_level_to_the_indent_the_line_already_has() {
    let mut editor = editor_in("a.rs", "    if x {");
    enter(&mut editor);
    assert_eq!(text(&editor), "    if x {\n        ");
}

#[test]
fn enter_after_each_kind_of_opening_bracket() {
    for opener in ["(", "[", "{"] {
        let mut editor = editor_in("a.rs", &format!("call{opener}"));
        enter(&mut editor);
        assert_eq!(text(&editor), format!("call{opener}\n    "), "{opener}");
    }
}

#[test]
fn enter_uses_the_files_indent_style() {
    let mut editor = styled("a.rs", "if x {", IndentStyle::Tabs);
    enter(&mut editor);
    assert_eq!(text(&editor), "if x {\n\t");

    let mut editor = styled("a.rs", "if x {", IndentStyle::Spaces(2));
    enter(&mut editor);
    assert_eq!(text(&editor), "if x {\n  ");
}

#[test]
fn enter_after_a_comment_that_ends_in_a_brace_does_not_indent() {
    let mut editor = editor_in("a.rs", "let x = 1; // opens {");
    enter(&mut editor);
    assert_eq!(text(&editor), "let x = 1; // opens {\n");
}

#[test]
fn enter_after_a_brace_followed_by_a_comment_indents() {
    let mut editor = editor_in("a.rs", "fn f() { // start");
    enter(&mut editor);
    assert_eq!(text(&editor), "fn f() { // start\n    ");
}

#[test]
fn enter_after_a_brace_inside_a_string_does_not_indent() {
    let mut editor = editor_in("a.rs", "let s = \"{\"");
    enter(&mut editor);
    assert_eq!(text(&editor), "let s = \"{\"\n");

    let mut editor = editor_in("a.rs", "let s = \"a \\\" {");
    enter(&mut editor);
    assert_eq!(text(&editor), "let s = \"a \\\" {\n");
}

#[test]
fn a_quote_in_a_character_literal_does_not_hide_the_brace() {
    let mut editor = editor_in("a.rs", "if c == '\"' {");
    enter(&mut editor);
    assert_eq!(text(&editor), "if c == '\"' {\n    ");
}

#[test]
fn enter_between_a_pair_puts_the_closer_on_its_own_line() {
    let mut editor = editor_in("a.rs", "fn main() {}");
    editor.move_cursor(Motion::Left).unwrap();
    enter(&mut editor);
    assert_eq!(text(&editor), "fn main() {\n    \n}");
    assert_eq!(head(&editor), Position::new(1, 4));
}

#[test]
fn enter_between_a_pair_keeps_the_outer_indent() {
    let mut editor = editor_in("a.rs", "    foo(Bar {})");
    editor.move_cursor(Motion::Left).unwrap();
    editor.move_cursor(Motion::Left).unwrap();
    enter(&mut editor);
    assert_eq!(text(&editor), "    foo(Bar {\n        \n    })");
    assert_eq!(head(&editor), Position::new(1, 8));
}

#[test]
fn the_middle_line_of_a_pair_is_undone_with_the_enter() {
    let mut editor = editor_in("a.rs", "{}");
    editor.move_cursor(Motion::Left).unwrap();
    enter(&mut editor);
    editor.undo().unwrap();
    assert_eq!(text(&editor), "{}");
}

#[test]
fn enter_with_a_different_closer_after_the_cursor_adds_no_line() {
    let mut editor = editor_in("a.rs", "{)");
    editor.move_cursor(Motion::Left).unwrap();
    enter(&mut editor);
    assert_eq!(text(&editor), "{\n    )");
}

#[test]
fn enter_in_the_middle_of_a_line_looks_only_left_of_the_cursor() {
    let mut editor = editor_in("a.rs", "a { b");
    editor.move_cursor(Motion::Left).unwrap();
    editor.move_cursor(Motion::Left).unwrap();
    enter(&mut editor);
    assert_eq!(text(&editor), "a {\n     b");
}

#[test]
fn enter_with_crlf_keeps_crlf() {
    let mut editor = editor_in("a.rs", "if x {\r\n");
    editor.move_cursor(Motion::Up).unwrap();
    editor.move_cursor(Motion::LineEnd).unwrap();
    enter(&mut editor);
    assert_eq!(text(&editor), "if x {\r\n    \r\n");
}

#[test]
fn enter_with_a_selection_replaces_it_and_indents_by_what_is_left_of_it() {
    let mut editor = editor_in("a.rs", "if x { abc");
    editor.select(Motion::Left).unwrap();
    editor.select(Motion::Left).unwrap();
    editor.select(Motion::Left).unwrap();
    enter(&mut editor);
    assert_eq!(text(&editor), "if x { \n    ");
}

// ---- languages

#[test]
fn json_toml_and_nix_indent_after_brackets() {
    for name in ["a.json", "a.toml", "a.nix"] {
        let mut editor = editor_in(name, "x = [");
        enter(&mut editor);
        assert_eq!(text(&editor), "x = [\n    ", "{name}");
    }
}

#[test]
fn hash_comments_hide_the_bracket_in_toml_and_nix() {
    for name in ["a.toml", "a.nix"] {
        let mut editor = editor_in(name, "x = 1 # [");
        enter(&mut editor);
        assert_eq!(text(&editor), "x = 1 # [\n", "{name}");
    }
}

#[test]
fn markdown_and_unknown_files_keep_copying_the_indent() {
    for name in ["a.md", "a.txt", "Makefile"] {
        let mut editor = editor_in(name, "  see (");
        enter(&mut editor);
        assert_eq!(text(&editor), "  see (\n  ", "{name}");
    }
}

#[test]
fn a_file_without_a_path_gets_no_smart_indent() {
    let mut editor = Editor::new(Buffer::from_text("fn f() {"));
    editor.move_cursor(Motion::DocumentEnd).unwrap();
    enter(&mut editor);
    assert_eq!(text(&editor), "fn f() {\n");
}

// ---- typing a closing bracket

#[test]
fn a_closer_typed_on_an_indent_only_line_takes_a_level_away() {
    let mut editor = editor_in("a.rs", "fn f() {\n    x();\n    ");
    type_str(&mut editor, "}");
    assert_eq!(text(&editor), "fn f() {\n    x();\n}");
    assert_eq!(head(&editor), Position::new(2, 1));
}

#[test]
fn a_closer_takes_away_to_the_previous_multiple_of_the_width() {
    let mut editor = editor_in("a.rs", "      ");
    type_str(&mut editor, ")");
    assert_eq!(text(&editor), "    )");
}

#[test]
fn a_closer_takes_away_one_tab() {
    let mut editor = styled("a.rs", "\t\t", IndentStyle::Tabs);
    type_str(&mut editor, "]");
    assert_eq!(text(&editor), "\t]");
}

#[test]
fn a_closer_on_a_line_with_code_stays_where_it_is() {
    let mut editor = editor_in("a.rs", "    x(");
    type_str(&mut editor, ")");
    assert_eq!(text(&editor), "    x()");
}

#[test]
fn a_closer_at_the_start_of_a_line_has_nothing_to_take() {
    let mut editor = editor_in("a.rs", "");
    type_str(&mut editor, "}");
    assert_eq!(text(&editor), "}");
}

#[test]
fn other_text_typed_on_an_indent_only_line_keeps_the_indent() {
    let mut editor = editor_in("a.rs", "    ");
    type_str(&mut editor, "x");
    assert_eq!(text(&editor), "    x");
}

#[test]
fn a_pasted_string_that_ends_in_a_closer_is_not_dedented() {
    let mut editor = editor_in("a.rs", "    ");
    type_str(&mut editor, "a}");
    assert_eq!(text(&editor), "    a}");
}

#[test]
fn a_closer_in_markdown_or_an_unnamed_file_is_not_dedented() {
    for name in ["a.md", "a.txt"] {
        let mut editor = editor_in(name, "    ");
        type_str(&mut editor, "}");
        assert_eq!(text(&editor), "    }", "{name}");
    }
}

#[test]
fn a_closer_replaces_a_selection_without_dedenting() {
    let mut editor = editor_in("a.rs", "    abc");
    editor.select(Motion::Left).unwrap();
    type_str(&mut editor, "}");
    assert_eq!(text(&editor), "    ab}");
}

#[test]
fn the_dedent_is_undone_with_the_closer() {
    let mut editor = editor_in("a.rs", "    ");
    type_str(&mut editor, "}");
    editor.undo().unwrap();
    assert_eq!(text(&editor), "    ");
}
