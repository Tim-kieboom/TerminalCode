use std::path::PathBuf;

use crate::buffer::{Buffer, Position, Selection};
use crate::components::editor::{Editor, Motion, Toggled};

/// An editor on `text` in a file called `name`, the cursor at the start.
fn editor_in(name: &str, text: &str) -> Editor {
    let mut buffer = Buffer::from_text(text);
    buffer.set_path(PathBuf::from(name));
    Editor::new(buffer)
}

fn rust(text: &str) -> Editor {
    editor_in("main.rs", text)
}

fn text(editor: &Editor) -> String {
    editor.buffer().text()
}

fn head(editor: &Editor) -> Position {
    editor.selections().primary().head()
}

fn goto(editor: &mut Editor, line: usize, column: usize) {
    editor.move_cursor(Motion::DocumentStart).unwrap();
    for _ in 0..line {
        editor.move_cursor(Motion::Down).unwrap();
    }
    editor.move_cursor(Motion::LineStart).unwrap();
    for _ in 0..column {
        editor.move_cursor(Motion::Right).unwrap();
    }
}

/// Selects from the start of line `from` to the end of line `to`.
fn select_lines(editor: &mut Editor, from: usize, to: usize) {
    goto(editor, from, 0);
    for _ in from..to {
        editor.select(Motion::Down).unwrap();
    }
    editor.select(Motion::LineEnd).unwrap();
}

fn toggle(editor: &mut Editor) -> Toggled {
    editor.toggle_comment().unwrap()
}

#[test]
fn a_line_is_commented_and_uncommented_and_the_cursor_goes_along() {
    let mut editor = rust("let x = 1;");
    goto(&mut editor, 0, 4);

    assert_eq!(toggle(&mut editor), Toggled::Done);
    assert_eq!(text(&editor), "// let x = 1;");
    assert_eq!(head(&editor), Position::new(0, 7));

    toggle(&mut editor);
    assert_eq!(text(&editor), "let x = 1;");
    assert_eq!(head(&editor), Position::new(0, 4));
}

#[test]
fn the_marker_goes_at_the_indentation_of_the_line() {
    let mut editor = rust("    let x = 1;");

    toggle(&mut editor);

    assert_eq!(text(&editor), "    // let x = 1;");
}

#[test]
fn a_mixed_selection_comments_all_of_it() {
    let mut editor = rust("// a\nb\n// c");
    select_lines(&mut editor, 0, 2);

    toggle(&mut editor);

    assert_eq!(text(&editor), "// // a\n// b\n// // c");
}

#[test]
fn a_selection_where_every_line_is_commented_uncomments_it() {
    let mut editor = rust("// a\n// b");
    select_lines(&mut editor, 0, 1);

    toggle(&mut editor);

    assert_eq!(text(&editor), "a\nb");
}

#[test]
fn blank_lines_are_left_alone_and_do_not_decide() {
    let mut editor = rust("// a\n\n   \n// b");
    select_lines(&mut editor, 0, 3);

    toggle(&mut editor);

    assert_eq!(
        text(&editor),
        "a\n\n   \nb",
        "the blank lines did not stop it uncommenting"
    );

    toggle(&mut editor);
    assert_eq!(text(&editor), "// a\n\n   \n// b");
}

#[test]
fn only_blank_lines_have_nothing_to_toggle() {
    let mut editor = rust("a\n\nb");
    goto(&mut editor, 1, 0);

    assert_eq!(toggle(&mut editor), Toggled::NothingToToggle);
    assert_eq!(text(&editor), "a\n\nb");
}

#[test]
fn mixed_indentation_puts_the_marker_at_the_smallest() {
    let mut editor = rust("fn f() {\n    a;\n        b;\n}");
    select_lines(&mut editor, 1, 2);

    toggle(&mut editor);

    assert_eq!(text(&editor), "fn f() {\n    // a;\n    //     b;\n}");

    toggle(&mut editor);
    assert_eq!(text(&editor), "fn f() {\n    a;\n        b;\n}");
}

#[test]
fn uncommenting_takes_one_space_only_when_there_is_one() {
    let mut editor = rust("//tight\n//  two");
    select_lines(&mut editor, 0, 1);

    toggle(&mut editor);

    assert_eq!(text(&editor), "tight\n two");
}

#[test]
fn toml_and_nix_use_a_hash() {
    for name in ["Cargo.toml", "flake.nix"] {
        let mut editor = editor_in(name, "key = 1");
        toggle(&mut editor);
        assert_eq!(text(&editor), "# key = 1", "{name}");
    }
}

#[test]
fn json_takes_double_slashes() {
    let mut editor = editor_in("settings.json", "\"a\": 1");

    toggle(&mut editor);

    assert_eq!(text(&editor), "// \"a\": 1");
}

#[test]
fn markdown_wraps_the_text_of_the_line() {
    let mut editor = editor_in("README.md", "  some prose  ");
    goto(&mut editor, 0, 4);

    toggle(&mut editor);
    assert_eq!(text(&editor), "  <!-- some prose -->  ");
    assert_eq!(head(&editor), Position::new(0, 9));

    toggle(&mut editor);
    assert_eq!(text(&editor), "  some prose  ");
    assert_eq!(head(&editor), Position::new(0, 4));
}

#[test]
fn markdown_comments_without_spaces_inside_are_taken_away_too() {
    let mut editor = editor_in("a.md", "<!--x-->\n<!---->");
    select_lines(&mut editor, 0, 1);

    toggle(&mut editor);

    assert_eq!(text(&editor), "x\n");
}

#[test]
fn a_file_with_no_language_says_so_and_changes_nothing() {
    let mut editor = editor_in("notes.txt", "hello");

    assert_eq!(toggle(&mut editor), Toggled::NoLanguage);
    assert_eq!(text(&editor), "hello");
}

#[test]
fn an_untitled_buffer_has_no_language_either() {
    let mut editor = Editor::new(Buffer::from_text("hello"));

    assert_eq!(toggle(&mut editor), Toggled::NoLanguage);
}

#[test]
fn cursors_on_different_lines_each_toggle_their_line_and_one_line_with_two_cursors_once() {
    let mut editor = rust("abcd\nxy\nz");
    goto(&mut editor, 0, 1);
    editor.add_cursor(Position::new(0, 3));
    editor.add_cursor(Position::new(2, 0));

    toggle(&mut editor);

    assert_eq!(text(&editor), "// abcd\nxy\n// z");
    let heads: Vec<Position> = editor.selections().iter().map(Selection::head).collect();
    assert_eq!(
        heads,
        vec![
            Position::new(0, 4),
            Position::new(0, 6),
            Position::new(2, 3)
        ]
    );
}

#[test]
fn the_toggle_is_one_undo_step() {
    let mut editor = rust("a\nb\nc");
    select_lines(&mut editor, 0, 2);

    toggle(&mut editor);
    editor.undo().unwrap();

    assert_eq!(text(&editor), "a\nb\nc");
}

#[test]
fn a_selection_that_ends_at_the_start_of_a_line_leaves_that_line_out() {
    let mut editor = rust("a\nb\nc");
    goto(&mut editor, 0, 0);
    editor.select(Motion::Down).unwrap();
    editor.select(Motion::Down).unwrap();

    toggle(&mut editor);

    assert_eq!(text(&editor), "// a\n// b\nc");
}
