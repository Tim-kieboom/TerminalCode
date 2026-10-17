use crate::buffer::{Buffer, Position, Selection};
use crate::clipboard::Register;
use crate::components::editor::{Editor, IndentKind, IndentPrefs, IndentStyle, Motion, TabWidth};

fn detect(text: &str) -> Option<IndentStyle> {
    IndentStyle::detect(&Buffer::from_text(text))
}

fn editor(text: &str) -> Editor {
    Editor::new(Buffer::from_text(text))
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

// ---- detection

#[test]
fn text_without_indentation_has_no_style() {
    assert_eq!(detect("a\nb\nc"), None);
    assert_eq!(detect(""), None);
}

#[test]
fn four_space_files_are_detected() {
    let code = "def f():\n    if x:\n        y()\n    z()\n";

    assert_eq!(detect(code), Some(IndentStyle::Spaces(4)));
}

#[test]
fn two_space_files_are_detected() {
    let code = "a:\n  b:\n    c: 1\n  d: 2\n";

    assert_eq!(detect(code), Some(IndentStyle::Spaces(2)));
}

#[test]
fn tab_files_are_detected() {
    let code = "fn main() {\n\tlet x = 1;\n\tif x {\n\t\tfoo();\n\t}\n}\n";

    assert_eq!(detect(code), Some(IndentStyle::Tabs));
}

#[test]
fn a_single_indented_line_is_enough_if_its_width_is_plausible() {
    assert_eq!(detect("a\n  b\n"), Some(IndentStyle::Spaces(2)));
    assert_eq!(detect("a\n   b\n"), Some(IndentStyle::Spaces(3)));
    // Eight spaces is more likely two levels than one wide level.
    assert_eq!(detect("a\n        b\n"), None);
}

#[test]
fn alignment_noise_is_outvoted_by_the_real_step() {
    let code = "f(\n    a,\n    b,\n)\nif x:\n    y\n   # odd comment\n";

    assert_eq!(detect(code), Some(IndentStyle::Spaces(4)));
}

#[test]
fn blank_lines_do_not_count() {
    assert_eq!(detect("a\n\n\n    b\n"), Some(IndentStyle::Spaces(4)));
}

#[test]
fn the_editor_uses_the_detected_style_or_four_spaces() {
    assert_eq!(editor("a\n  b\n").view().indent(), IndentStyle::Spaces(2));
    assert_eq!(editor("a\n\tb\n").view().indent(), IndentStyle::Tabs);
    assert_eq!(editor("plain").view().indent(), IndentStyle::Spaces(4));
    assert_eq!(Editor::default().view().indent(), IndentStyle::Spaces(4));
}

// ---- Enter

#[test]
fn enter_copies_the_leading_whitespace_of_the_line() {
    let mut editor = editor("    foo");
    editor.move_cursor(Motion::LineEnd).unwrap();

    editor.insert_newline().unwrap();

    assert_eq!(text(&editor), "    foo\n    ");
    assert_eq!(head(&editor), Position::new(1, 4));
}

#[test]
fn enter_copies_tabs_exactly() {
    let mut editor = editor("\t\tfoo");
    editor.move_cursor(Motion::LineEnd).unwrap();

    editor.insert_newline().unwrap();

    assert_eq!(text(&editor), "\t\tfoo\n\t\t");
}

#[test]
fn enter_in_the_middle_of_the_indent_copies_only_the_part_left_of_the_cursor() {
    let mut editor = editor("        foo");
    goto(&mut editor, 0, 3);

    editor.insert_newline().unwrap();

    assert_eq!(text(&editor), "   \n        foo");
    assert_eq!(head(&editor), Position::new(1, 3));
}

#[test]
fn enter_on_an_unindented_line_adds_no_indent() {
    let mut editor = editor("foo");
    editor.move_cursor(Motion::LineEnd).unwrap();

    editor.insert_newline().unwrap();

    assert_eq!(text(&editor), "foo\n");
}

#[test]
fn enter_with_crlf_keeps_crlf_and_the_indent() {
    let mut editor = editor("  a\r\nb");
    editor.move_cursor(Motion::LineEnd).unwrap();

    editor.insert_newline().unwrap();

    assert_eq!(text(&editor), "  a\r\n  \r\nb");
}

#[test]
fn enter_replaces_a_selection_and_indents_from_its_start() {
    let mut editor = editor("  abc def");
    goto(&mut editor, 0, 5);
    for _ in 0..3 {
        editor.select(Motion::Right).unwrap();
    }

    editor.insert_newline().unwrap();

    assert_eq!(text(&editor), "  abc\n  f");
    assert_eq!(head(&editor), Position::new(1, 2));
}

#[test]
fn the_auto_indent_is_one_undo_step() {
    let mut editor = editor("    foo");
    editor.move_cursor(Motion::LineEnd).unwrap();
    editor.insert_newline().unwrap();

    editor.undo().unwrap();

    assert_eq!(text(&editor), "    foo");
}

// ---- Tab

#[test]
fn tab_inserts_spaces_up_to_the_next_tab_stop() {
    let mut editor = editor("ab");
    editor.move_cursor(Motion::LineEnd).unwrap();

    editor.indent().unwrap();

    assert_eq!(text(&editor), "ab  ");
    assert_eq!(head(&editor), Position::new(0, 4));
}

#[test]
fn tab_at_a_tab_stop_inserts_a_full_unit() {
    let mut editor = editor("");

    editor.indent().unwrap();

    assert_eq!(text(&editor), "    ");
}

#[test]
fn tab_uses_a_real_tab_in_a_tab_file() {
    let mut editor = editor("a\n\tb\n");
    goto(&mut editor, 1, 1);

    editor.indent().unwrap();

    assert_eq!(text(&editor), "a\n\t\tb\n");
}

#[test]
fn tab_uses_two_spaces_in_a_two_space_file() {
    let mut editor = editor("a\n  b\n");
    goto(&mut editor, 0, 1);

    editor.indent().unwrap();

    assert_eq!(text(&editor), "a \n  b\n");
}

#[test]
fn tab_with_a_selection_on_one_line_replaces_it() {
    let mut editor = editor("abcdef");
    goto(&mut editor, 0, 1);
    for _ in 0..2 {
        editor.select(Motion::Right).unwrap();
    }

    editor.indent().unwrap();

    assert_eq!(text(&editor), "a   def");
}

#[test]
fn tab_with_a_multiline_selection_indents_every_line() {
    let mut editor = editor("a\nb\nc\nd");
    select_lines(&mut editor, 1, 2);

    editor.indent().unwrap();

    assert_eq!(text(&editor), "a\n    b\n    c\nd");
}

#[test]
fn block_indent_skips_empty_lines_and_a_last_line_the_selection_only_touches() {
    let mut editor = editor("a\n\nb\nc");
    goto(&mut editor, 0, 0);
    editor.select(Motion::Down).unwrap();
    editor.select(Motion::Down).unwrap();
    // The selection ends at column 0 of line 2, so line 2 stays as is.

    editor.indent().unwrap();

    assert_eq!(text(&editor), "    a\n\nb\nc");
}

#[test]
fn block_indent_keeps_the_selection_around_the_same_text() {
    let mut editor = editor("abc\ndef");
    goto(&mut editor, 0, 1);
    editor.select(Motion::Down).unwrap();
    editor.select(Motion::Right).unwrap();

    editor.indent().unwrap();

    let selection = *editor.selections().primary();
    assert_eq!(selection.start(), Position::new(0, 5));
    assert_eq!(selection.end(), Position::new(1, 6));
}

#[test]
fn block_indent_is_one_undo_step() {
    let mut editor = editor("a\nb\nc");
    select_lines(&mut editor, 0, 2);
    editor.indent().unwrap();

    editor.undo().unwrap();

    assert_eq!(text(&editor), "a\nb\nc");
}

// ---- Shift+Tab

#[test]
fn outdent_removes_one_unit_from_the_current_line() {
    let mut editor = editor("        foo");
    editor.move_cursor(Motion::LineEnd).unwrap();

    editor.outdent().unwrap();

    assert_eq!(text(&editor), "    foo");
    assert_eq!(head(&editor), Position::new(0, 7));
}

#[test]
fn outdent_removes_only_what_is_there() {
    let mut editor = editor("  foo");

    editor.outdent().unwrap();

    assert_eq!(text(&editor), "foo");
}

#[test]
fn outdent_removes_a_tab_from_a_tab_file() {
    let mut editor = editor("a\n\t\tb\n");
    goto(&mut editor, 1, 2);

    editor.outdent().unwrap();

    assert_eq!(text(&editor), "a\n\tb\n");
    assert_eq!(head(&editor), Position::new(1, 1));
}

#[test]
fn outdent_on_an_unindented_line_does_nothing() {
    let mut editor = editor("foo");

    editor.outdent().unwrap();

    assert_eq!(text(&editor), "foo");
    assert_eq!(editor.buffer().version(), 0);
}

#[test]
fn outdent_with_a_selection_outdents_every_selected_line() {
    let mut editor = editor("    a\n    b\n    c");
    select_lines(&mut editor, 0, 1);

    editor.outdent().unwrap();

    assert_eq!(text(&editor), "a\nb\n    c");
}

#[test]
fn a_cursor_inside_the_removed_indent_moves_to_the_line_start() {
    let mut editor = editor("        foo");
    goto(&mut editor, 0, 2);

    editor.outdent().unwrap();

    assert_eq!(head(&editor), Position::new(0, 0));
}

#[test]
fn indent_then_outdent_round_trips() {
    let mut editor = editor("a\nb\nc");
    select_lines(&mut editor, 0, 2);

    editor.indent().unwrap();
    editor.outdent().unwrap();

    assert_eq!(text(&editor), "a\nb\nc");
}

// ---- paste stays verbatim

#[test]
fn pasting_indented_code_does_not_re_indent_it() {
    let code = "def greet():\n    print(\"Hello\")\n    if True:\n        print(\"Welcome\")\n";
    let mut editor = editor("");

    editor.paste(&Register::charwise(code)).unwrap();

    assert_eq!(text(&editor), code);
}

#[test]
fn pasting_after_indented_text_keeps_the_pasted_indentation_as_is() {
    let mut editor = editor("    x = [");
    editor.move_cursor(Motion::LineEnd).unwrap();

    editor
        .paste(&Register::charwise("\n        1,\n    ]"))
        .unwrap();

    assert_eq!(text(&editor), "    x = [\n        1,\n    ]");
}

// ---- what the settings say

fn prefs(kind: IndentKind, size: usize, detect: bool) -> IndentPrefs {
    IndentPrefs {
        kind,
        size: TabWidth::new(size).unwrap(),
        detect,
    }
}

fn style_for(prefs: IndentPrefs, text: &str) -> IndentStyle {
    prefs.style_for(&Buffer::from_text(text))
}

#[test]
fn the_default_preferences_are_what_the_editor_always_did() {
    let defaults = IndentPrefs::default();

    assert_eq!(style_for(defaults, "plain\n"), IndentStyle::Spaces(4));
    assert_eq!(style_for(defaults, "a\n  b\n"), IndentStyle::Spaces(2));
    assert_eq!(style_for(defaults, "a\n\tb\n"), IndentStyle::Tabs);
}

#[test]
fn a_file_with_no_indentation_gets_the_style_and_size_from_the_settings() {
    assert_eq!(
        style_for(prefs(IndentKind::Spaces, 2, true), "plain\n"),
        IndentStyle::Spaces(2)
    );
    assert_eq!(
        style_for(prefs(IndentKind::Spaces, 8, true), ""),
        IndentStyle::Spaces(8)
    );
    assert_eq!(
        style_for(prefs(IndentKind::Tabs, 4, true), "plain\n"),
        IndentStyle::Tabs
    );
}

#[test]
fn detection_keeps_the_style_a_file_already_has() {
    assert_eq!(
        style_for(prefs(IndentKind::Tabs, 8, true), "a\n  b\n"),
        IndentStyle::Spaces(2)
    );
    assert_eq!(
        style_for(prefs(IndentKind::Spaces, 2, true), "a\n\tb\n"),
        IndentStyle::Tabs
    );
}

#[test]
fn with_detection_off_the_settings_win_over_what_the_file_has() {
    assert_eq!(
        style_for(prefs(IndentKind::Spaces, 8, false), "a\n  b\n"),
        IndentStyle::Spaces(8)
    );
    assert_eq!(
        style_for(prefs(IndentKind::Tabs, 4, false), "a\n    b\n"),
        IndentStyle::Tabs
    );
}

// ---- the line the cursor is on: indent_line and outdent_line

#[test]
fn indent_line_moves_the_whole_line_from_a_cursor_in_the_middle_and_the_cursor_goes_along() {
    let mut editor = editor("hello world");
    goto(&mut editor, 0, 6);

    editor.indent_line().unwrap();

    assert_eq!(text(&editor), "    hello world");
    assert_eq!(head(&editor), Position::new(0, 10), "still before `world`");
}

#[test]
fn indent_line_at_the_start_and_end_of_a_line() {
    let mut editor = editor("ab");
    goto(&mut editor, 0, 0);
    editor.indent_line().unwrap();
    assert_eq!(head(&editor), Position::new(0, 4), "before the `a` still");

    editor.move_cursor(Motion::LineEnd).unwrap();
    editor.indent_line().unwrap();
    assert_eq!(text(&editor), "        ab");
    assert_eq!(head(&editor), Position::new(0, 10));
}

#[test]
fn outdent_line_takes_one_level_from_a_cursor_in_the_middle() {
    let mut editor = editor("        hello");
    goto(&mut editor, 0, 10);

    editor.outdent_line().unwrap();

    assert_eq!(text(&editor), "    hello");
    assert_eq!(head(&editor), Position::new(0, 6));
}

#[test]
fn outdent_line_takes_less_than_a_level_when_there_is_less_and_nothing_when_none() {
    let mut editor = editor("  two\nnone");
    goto(&mut editor, 0, 3);
    editor.outdent_line().unwrap();
    assert_eq!(text(&editor), "two\nnone");
    assert_eq!(head(&editor), Position::new(0, 1));

    goto(&mut editor, 1, 2);
    editor.outdent_line().unwrap();
    assert_eq!(text(&editor), "two\nnone", "nothing to take");
    assert_eq!(head(&editor), Position::new(1, 2));
}

#[test]
fn a_tab_file_moves_by_tabs() {
    let mut editor = editor("\tone\n\ttwo\n");
    goto(&mut editor, 0, 0);
    editor.indent_line().unwrap();
    assert_eq!(text(&editor), "\t\tone\n\ttwo\n");

    editor.outdent_line().unwrap();
    editor.outdent_line().unwrap();
    assert_eq!(text(&editor), "one\n\ttwo\n");
}

#[test]
fn a_selection_over_three_lines_moves_every_line_and_blank_ones_stay_blank() {
    let mut editor = editor("a\n\nb\nc");
    select_lines(&mut editor, 0, 2);

    editor.indent_line().unwrap();

    assert_eq!(text(&editor), "    a\n\n    b\nc");
    let selection = *editor.selections().primary();
    assert_eq!(
        selection.start(),
        Position::new(0, 0),
        "still covers the new indent"
    );
    assert_eq!(selection.end(), Position::new(2, 5));

    editor.outdent_line().unwrap();
    assert_eq!(text(&editor), "a\n\nb\nc");
}

#[test]
fn a_blank_cursor_line_is_indented_when_it_is_the_only_line() {
    let mut editor = editor("a\n\nb");
    goto(&mut editor, 1, 0);

    editor.indent_line().unwrap();

    assert_eq!(text(&editor), "a\n    \nb");
}

#[test]
fn two_cursors_on_one_line_move_it_once_and_both_go_along() {
    let mut editor = editor("abcd\nxy");
    goto(&mut editor, 0, 1);
    editor.add_cursor(Position::new(0, 3));

    editor.indent_line().unwrap();

    assert_eq!(text(&editor), "    abcd\nxy");
    let heads: Vec<Position> = editor.selections().iter().map(Selection::head).collect();
    assert_eq!(heads, vec![Position::new(0, 5), Position::new(0, 7)]);
}

#[test]
fn cursors_on_different_lines_each_move_their_line() {
    let mut editor = editor("a\nb\nc");
    goto(&mut editor, 0, 1);
    editor.add_cursor(Position::new(2, 1));

    editor.indent_line().unwrap();

    assert_eq!(text(&editor), "    a\nb\n    c");
    assert_eq!(editor.selections().len(), 2);
}

#[test]
fn the_move_is_one_undo_step() {
    let mut editor = editor("a\nb\nc");
    select_lines(&mut editor, 0, 2);

    editor.indent_line().unwrap();
    editor.undo().unwrap();

    assert_eq!(text(&editor), "a\nb\nc");
}

#[test]
fn a_cursor_on_a_blank_line_moves_with_the_cursors_on_other_lines() {
    let mut editor = editor("foo()\n\nbar()");
    goto(&mut editor, 0, 0);
    editor.add_cursor(Position::new(1, 0));

    editor.indent_line().unwrap();

    assert_eq!(text(&editor), "    foo()\n    \nbar()");
}

#[test]
fn a_blank_line_inside_a_selected_block_stays_blank() {
    let mut editor = editor("a\n\nb");
    select_lines(&mut editor, 0, 2);

    editor.indent_line().unwrap();

    assert_eq!(text(&editor), "    a\n\n    b");
}

#[test]
fn tab_and_shift_tab_over_a_selection_use_the_same_lines() {
    let mut editor = editor("a\n\nb\nc");
    select_lines(&mut editor, 0, 2);

    editor.indent().unwrap();
    assert_eq!(text(&editor), "    a\n\n    b\nc");

    editor.outdent().unwrap();
    assert_eq!(text(&editor), "a\n\nb\nc");
}
