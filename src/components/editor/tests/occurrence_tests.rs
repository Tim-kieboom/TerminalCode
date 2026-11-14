//! `ctrl+d` (the next occurrence), and what Enter, a closing bracket, Tab and Shift+Tab do with
//! several cursors.

use std::path::PathBuf;

use crate::buffer::{Buffer, Position};
use crate::components::editor::{Editor, IndentStyle, Occurrence, ViewState};

fn editor_in(name: &str, text: &str) -> Editor {
    let mut buffer = Buffer::from_text(text);
    buffer.set_path(PathBuf::from(name));
    Editor::from_parts(ViewState::with_indent(IndentStyle::Spaces(4)), buffer)
}

fn text(editor: &Editor) -> String {
    editor.buffer().text()
}

/// The cursors as (line, column), in text order.
fn heads(editor: &Editor) -> Vec<(usize, usize)> {
    editor
        .selections()
        .iter()
        .map(|selection| (selection.head().line, selection.head().column))
        .collect()
}

/// An editor with a cursor at each of `places`; the last is the primary one.
fn cursors_at(name: &str, source: &str, places: &[(usize, usize)]) -> Editor {
    let mut editor = editor_in(name, source);
    editor.go_to(Position::new(places[0].0, places[0].1));
    for place in &places[1..] {
        editor.add_cursor(Position::new(place.0, place.1));
    }
    editor
}

fn select(editor: &mut Editor, from: (usize, usize), to: (usize, usize)) {
    editor.select_range(Position::new(from.0, from.1), Position::new(to.0, to.1));
}

/// The text the primary selection covers.
fn selected(editor: &Editor) -> String {
    let selection = editor.selections().primary();
    let buffer = editor.buffer();
    let start = buffer.position_to_byte(selection.start()).unwrap();
    let end = buffer.position_to_byte(selection.end()).unwrap();
    buffer.text_in(start..end).unwrap()
}

// ---- ctrl+d

#[test]
fn a_bare_cursor_in_a_word_selects_the_word() {
    let mut editor = editor_in("a.rs", "let total = total + 1;");
    editor.go_to(Position::new(0, 6));

    assert_eq!(editor.select_next_occurrence().unwrap(), Occurrence::Word);

    assert_eq!(heads(&editor), [(0, 9)]);
    assert_eq!(selected(&editor), "total");
}

#[test]
fn a_cursor_at_the_end_of_a_word_selects_that_word() {
    let mut editor = editor_in("a.rs", "foo bar");
    editor.go_to(Position::new(0, 3));

    assert_eq!(editor.select_next_occurrence().unwrap(), Occurrence::Word);

    assert_eq!(selected(&editor), "foo");
}

#[test]
fn a_cursor_that_is_not_by_a_word_selects_nothing() {
    let mut editor = editor_in("a.rs", "foo   bar");
    editor.go_to(Position::new(0, 4));

    assert_eq!(editor.select_next_occurrence().unwrap(), Occurrence::NoWord);
    assert_eq!(editor.selections().len(), 1);
}

#[test]
fn pressing_again_adds_the_next_occurrence_and_it_becomes_the_primary_one() {
    let mut editor = editor_in("a.rs", "foo bar foo baz foo");
    editor.go_to(Position::new(0, 1));
    editor.select_next_occurrence().unwrap();

    assert_eq!(editor.select_next_occurrence().unwrap(), Occurrence::Added);
    assert_eq!(heads(&editor), [(0, 3), (0, 11)]);
    assert_eq!(editor.selections().primary().head(), Position::new(0, 11));

    assert_eq!(editor.select_next_occurrence().unwrap(), Occurrence::Added);
    assert_eq!(heads(&editor), [(0, 3), (0, 11), (0, 19)]);
}

#[test]
fn it_goes_round_to_the_start_and_then_says_there_are_no_more() {
    let mut editor = editor_in("a.rs", "foo bar foo");
    editor.go_to(Position::new(0, 9));
    editor.select_next_occurrence().unwrap();

    assert_eq!(editor.select_next_occurrence().unwrap(), Occurrence::Added);
    assert_eq!(
        heads(&editor),
        [(0, 3), (0, 11)],
        "the first one, round the end"
    );
    assert_eq!(editor.select_next_occurrence().unwrap(), Occurrence::NoMore);
    assert_eq!(editor.selections().len(), 2);
}

#[test]
fn a_word_does_not_match_inside_a_longer_one_but_other_text_does() {
    let mut editor = editor_in("a.rs", "foo foobar foo");
    editor.go_to(Position::new(0, 1));
    editor.select_next_occurrence().unwrap();
    editor.select_next_occurrence().unwrap();
    assert_eq!(heads(&editor), [(0, 3), (0, 14)], "not the one in foobar");

    let mut editor = editor_in("a.rs", "a.b a.bc");
    select(&mut editor, (0, 0), (0, 3));
    editor.select_next_occurrence().unwrap();
    assert_eq!(
        heads(&editor),
        [(0, 3), (0, 7)],
        "text that is no word matches in `a.bc`"
    );
}

#[test]
fn a_selection_over_several_lines_finds_its_text_again() {
    let mut editor = editor_in("a.rs", "x\ny\n--\nx\ny\n");
    select(&mut editor, (0, 0), (1, 1));

    assert_eq!(editor.select_next_occurrence().unwrap(), Occurrence::Added);

    assert_eq!(heads(&editor), [(1, 1), (4, 1)]);
}

#[test]
fn typing_after_it_replaces_every_selected_occurrence_in_one_undo_step() {
    let mut editor = editor_in("a.rs", "foo bar foo baz foo");
    editor.go_to(Position::new(0, 1));
    editor.select_next_occurrence().unwrap();
    editor.select_next_occurrence().unwrap();
    editor.select_next_occurrence().unwrap();

    editor.insert_text("qux").unwrap();

    assert_eq!(text(&editor), "qux bar qux baz qux");
    editor.undo().unwrap();
    assert_eq!(text(&editor), "foo bar foo baz foo");
}

#[test]
fn with_other_cursors_around_only_the_primary_one_picks_the_word() {
    let mut editor = cursors_at("a.rs", "one two\nthree", &[(0, 1), (1, 1)]);

    editor.select_next_occurrence().unwrap();

    assert_eq!(editor.selections().len(), 2, "the other cursor is kept");
    assert_eq!(selected(&editor), "three");
}

// ---- select all occurrences, skip, take back

#[test]
fn select_all_takes_every_whole_word_and_leaves_the_primary_one_where_it_was() {
    let mut editor = editor_in("a.rs", "foo foobar foo foo");
    editor.go_to(Position::new(0, 1));

    assert_eq!(editor.select_all_occurrences().unwrap(), Occurrence::All(2));

    assert_eq!(heads(&editor), [(0, 3), (0, 14), (0, 18)], "not the foobar");
    assert_eq!(
        editor.selections().primary().head(),
        Position::new(0, 3),
        "the word the cursor was in"
    );
}

#[test]
fn select_all_does_not_add_what_is_selected_and_then_says_so() {
    let mut editor = editor_in("a.rs", "foo bar foo");
    editor.go_to(Position::new(0, 1));
    editor.select_next_occurrence().unwrap();
    editor.select_next_occurrence().unwrap();

    assert_eq!(editor.select_all_occurrences().unwrap(), Occurrence::NoMore);
}

#[test]
fn skip_moves_the_primary_selection_to_the_next_occurrence() {
    let mut editor = editor_in("a.rs", "foo bar foo baz foo");
    editor.go_to(Position::new(0, 1));
    editor.select_next_occurrence().unwrap();
    editor.select_next_occurrence().unwrap();
    assert_eq!(heads(&editor), [(0, 3), (0, 11)]);

    assert_eq!(editor.skip_occurrence().unwrap(), Occurrence::Skipped);

    assert_eq!(
        heads(&editor),
        [(0, 3), (0, 19)],
        "the one at 11 is left out"
    );
    assert_eq!(editor.selections().primary().head(), Position::new(0, 19));
}

#[test]
fn skip_with_nowhere_to_go_changes_nothing() {
    let mut editor = editor_in("a.rs", "foo bar");
    editor.go_to(Position::new(0, 1));
    editor.select_next_occurrence().unwrap();

    assert_eq!(editor.skip_occurrence().unwrap(), Occurrence::NoMore);

    assert_eq!(heads(&editor), [(0, 3)]);
}

#[test]
fn take_back_drops_the_newest_selection_and_the_one_before_it_is_primary() {
    let mut editor = editor_in("a.rs", "foo bar foo baz foo");
    editor.go_to(Position::new(0, 1));
    editor.select_next_occurrence().unwrap();
    editor.select_next_occurrence().unwrap();
    editor.select_next_occurrence().unwrap();

    assert!(editor.take_back_selection());

    assert_eq!(heads(&editor), [(0, 3), (0, 11)]);
    assert_eq!(editor.selections().primary().head(), Position::new(0, 11));
    assert!(editor.take_back_selection());
    assert!(!editor.take_back_selection(), "one is left, and it stays");
    assert_eq!(heads(&editor), [(0, 3)]);
}

// ---- Enter with several cursors

#[test]
fn enter_at_every_cursor_after_an_opening_bracket_goes_one_level_deeper() {
    let mut editor = cursors_at("a.rs", "fn a() {\nfn b() {", &[(0, 8), (1, 8)]);

    editor.insert_newline().unwrap();

    assert_eq!(text(&editor), "fn a() {\n    \nfn b() {\n    ");
    assert_eq!(heads(&editor), [(1, 4), (3, 4)]);
}

#[test]
fn enter_between_a_bracket_pair_at_every_cursor_opens_a_line_for_each_closer() {
    let mut editor = cursors_at("a.rs", "a {}\nb {}", &[(0, 3), (1, 3)]);

    editor.insert_newline().unwrap();

    assert_eq!(text(&editor), "a {\n    \n}\nb {\n    \n}");
    assert_eq!(
        heads(&editor),
        [(1, 4), (4, 4)],
        "on the empty lines in the middle"
    );
    editor.insert_text("x").unwrap();
    assert_eq!(text(&editor), "a {\n    x\n}\nb {\n    x\n}");
}

#[test]
fn enter_at_cursors_with_different_indentation_keeps_each_ones_own() {
    let mut editor = cursors_at("a.rs", "fn a() {\n    let x = 1;\n}", &[(0, 8), (1, 14)]);

    editor.insert_newline().unwrap();

    assert_eq!(text(&editor), "fn a() {\n    \n    let x = 1;\n    \n}");
}

#[test]
fn enter_in_a_file_with_no_brackets_copies_the_indentation_at_every_cursor() {
    let mut editor = cursors_at("a.md", "  one {\n  two {", &[(0, 7), (1, 7)]);

    editor.insert_newline().unwrap();

    assert_eq!(text(&editor), "  one {\n  \n  two {\n  ");
}

// ---- a closing bracket with several cursors

#[test]
fn a_closing_bracket_on_a_line_of_indentation_takes_a_level_away_at_every_cursor() {
    let mut editor = cursors_at("a.rs", "    \n        ", &[(0, 4), (1, 8)]);

    editor.insert_text("}").unwrap();

    assert_eq!(text(&editor), "}\n    }");
    assert_eq!(heads(&editor), [(0, 1), (1, 5)]);
}

#[test]
fn a_closing_bracket_after_code_is_just_typed_at_every_cursor() {
    let mut editor = cursors_at("a.rs", "x\ny", &[(0, 1), (1, 1)]);

    editor.insert_text(")").unwrap();

    assert_eq!(text(&editor), "x)\ny)");
}

// ---- Tab and Shift+Tab with several cursors

#[test]
fn tab_puts_a_level_at_every_cursor_up_to_its_own_tab_stop() {
    let mut editor = cursors_at("a.txt", "ab\nabcdef", &[(0, 2), (1, 5)]);

    editor.indent().unwrap();

    assert_eq!(text(&editor), "ab  \nabcde   f");
    assert_eq!(heads(&editor), [(0, 4), (1, 8)]);
}

#[test]
fn tab_with_tabs_as_the_style_puts_a_tab_at_every_cursor() {
    let mut buffer = Buffer::from_text("a\nb");
    buffer.set_path(PathBuf::from("a.txt"));
    let mut editor = Editor::from_parts(ViewState::with_indent(IndentStyle::Tabs), buffer);
    editor.add_cursor(Position::new(1, 0));

    editor.indent().unwrap();

    assert_eq!(text(&editor), "\ta\n\tb");
}

#[test]
fn tab_in_one_undo_step() {
    let mut editor = cursors_at("a.txt", "a\nb", &[(0, 1), (1, 1)]);

    editor.indent().unwrap();
    editor.undo().unwrap();

    assert_eq!(text(&editor), "a\nb");
}

#[test]
fn tab_with_a_selection_over_several_lines_indents_those_lines_and_the_other_cursors_lines() {
    let mut editor = cursors_at("a.txt", "a\nb\nc\nd", &[(3, 0)]);
    select(&mut editor, (0, 0), (1, 1));
    editor.add_cursor(Position::new(3, 0));

    editor.indent().unwrap();

    assert_eq!(text(&editor), "    a\n    b\nc\n    d");
    assert_eq!(editor.selections().len(), 2, "the cursors stay");
}

#[test]
fn shift_tab_takes_a_level_off_the_line_of_every_cursor() {
    let mut editor = cursors_at("a.txt", "    a\n  b\nc", &[(0, 5), (1, 3), (2, 1)]);

    editor.outdent().unwrap();

    assert_eq!(text(&editor), "a\nb\nc");
    assert_eq!(editor.selections().len(), 3);
}

#[test]
fn tab_and_shift_tab_with_one_cursor_are_as_before() {
    let mut editor = editor_in("a.txt", "ab");
    editor.go_to(Position::new(0, 2));

    editor.indent().unwrap();
    assert_eq!(text(&editor), "ab  ");
    editor.go_to(Position::new(0, 0));
    editor.indent().unwrap();
    assert_eq!(text(&editor), "    ab  ");
    editor.outdent().unwrap();
    assert_eq!(text(&editor), "ab  ", "a level off the start of the line");
}
