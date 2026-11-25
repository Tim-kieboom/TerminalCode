use ratatui::backend::TestBackend;
use ratatui::{Frame, Terminal};

use crate::app::state::AppState;
use crate::buffer::{Buffer, Position};
use crate::components::editor::matching::{Matched, find};
use crate::components::editor::{Editor, Motion};
use crate::ui::ComponentKind;
use crate::ui::Render;
use crate::ui::layout::Placement;

fn buffer(text: &str) -> Buffer {
    Buffer::from_text(text)
}

fn found(text: &str, line: usize, column: usize) -> Option<Matched> {
    find(&buffer(text), Position::new(line, column), 50)
}

#[test]
fn a_bracket_after_the_cursor_finds_its_closer() {
    let matched = found("f(a, (b))", 0, 1).unwrap();
    assert_eq!(matched.bracket, Position::new(0, 1));
    assert_eq!(matched.other, Position::new(0, 8));
    assert!(!matched.cursor_after);
}

#[test]
fn a_bracket_before_the_cursor_is_used_when_none_is_after_it() {
    let matched = found("f(a)", 0, 4).unwrap();
    assert_eq!(matched.bracket, Position::new(0, 3));
    assert_eq!(matched.other, Position::new(0, 1));
    assert!(matched.cursor_after);
}

#[test]
fn the_bracket_after_the_cursor_wins_over_the_one_before_it() {
    let matched = found("()[]", 0, 2).unwrap();
    assert_eq!(matched.bracket, Position::new(0, 2));
    assert_eq!(matched.other, Position::new(0, 3));
}

#[test]
fn nesting_of_the_same_kind_is_counted_and_other_kinds_are_not() {
    assert_eq!(
        found("{ ( { } ) }", 0, 0).unwrap().other,
        Position::new(0, 10)
    );
    assert_eq!(found("( ] )", 0, 0).unwrap().other, Position::new(0, 4));
    assert_eq!(found("(( ) )", 0, 5).unwrap().other, Position::new(0, 0));
}

#[test]
fn a_match_on_another_line_is_found_both_ways() {
    let text = "fn f() {\n    x\n}\n";
    assert_eq!(found(text, 0, 7).unwrap().other, Position::new(2, 0));
    assert_eq!(found(text, 2, 0).unwrap().other, Position::new(0, 7));
}

#[test]
fn no_bracket_next_to_the_cursor_or_no_match_gives_none() {
    assert_eq!(found("abc", 0, 1), None);
    assert_eq!(found("(abc", 0, 0), None);
    assert_eq!(found("abc)", 0, 3), None);
    assert_eq!(found("", 0, 0), None);
}

#[test]
fn the_search_stops_after_the_lines_it_may_look_at() {
    let text = format!("(\n{}\n)", "x\n".repeat(20));
    assert!(find(&buffer(&text), Position::new(0, 0), 5).is_none());
    assert!(find(&buffer(&text), Position::new(0, 0), 50).is_some());
}

#[test]
fn a_bracket_after_wide_characters_is_found_by_grapheme_column() {
    let matched = found("\u{3053}\u{3053}(a)", 0, 2).unwrap();
    assert_eq!(matched.other, Position::new(0, 4));
}

#[test]
fn a_line_that_is_too_long_is_not_searched() {
    let long = format!("({})", "x".repeat(150_000));
    assert!(found(&long, 0, 0).is_none());
}

#[test]
fn jumping_goes_to_the_same_side_of_the_match_and_back() {
    let mut editor = Editor::new(buffer("f(a)"));
    editor.move_cursor(Motion::DocumentStart).unwrap();
    editor.move_cursor(Motion::Right).unwrap();
    // Before the `(`: goes before the `)`.
    assert_eq!(
        editor.jump_to_matching_bracket().unwrap(),
        Some(Position::new(0, 3))
    );
    assert_eq!(
        editor.jump_to_matching_bracket().unwrap(),
        Some(Position::new(0, 1))
    );
    // After a `)`: goes after the `(`.
    editor.move_cursor(Motion::DocumentEnd).unwrap();
    assert_eq!(
        editor.jump_to_matching_bracket().unwrap(),
        Some(Position::new(0, 2))
    );
}

#[test]
fn jumping_without_a_match_leaves_the_cursor() {
    let mut editor = Editor::new(buffer("(a"));
    editor.move_cursor(Motion::DocumentStart).unwrap();
    assert_eq!(editor.jump_to_matching_bracket().unwrap(), None);
    assert_eq!(editor.selections().primary().head(), Position::new(0, 0));
}

#[test]
fn a_selection_has_no_matching_bracket() {
    let mut editor = Editor::new(buffer("(a)"));
    editor.select_range(Position::new(0, 0), Position::new(0, 2));
    assert!(editor.matching_bracket(10).is_none());
}

fn drawn_backgrounds(text: &str, cursor_column: usize) -> Vec<Option<ratatui::style::Color>> {
    let mut state = AppState::new(Editor::new(buffer(text)));
    state.edit(|editor| {
        editor.move_cursor(Motion::DocumentStart).unwrap();
        for _ in 0..cursor_column {
            editor.move_cursor(Motion::Right).unwrap();
        }
    });
    let mut terminal = Terminal::new(TestBackend::new(30, 6)).unwrap();
    terminal
        .draw(|frame: &mut Frame| {
            let placement = Placement::new(ComponentKind::Editor, frame.area());
            state.workspace_mut().prepare(&placement);
            state.workspace().render(frame, &state, &placement);
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    // The text starts after the border and the 4 wide gutter, on the second row below the tab bar.
    (0..text.chars().count() as u16)
        .map(|column| buffer[(5 + column, 2)].bg)
        .map(Some)
        .collect()
}

#[test]
fn both_brackets_are_drawn_in_the_bracket_style_and_the_rest_is_not() {
    let backgrounds = drawn_backgrounds("f(a)", 1);
    assert_eq!(backgrounds[1], backgrounds[3]);
    assert_ne!(backgrounds[1], backgrounds[0]);
    assert_ne!(backgrounds[1], backgrounds[2]);
}

#[test]
fn no_bracket_is_marked_when_the_cursor_is_not_next_to_one() {
    let backgrounds = drawn_backgrounds("f(a)", 0);
    assert_eq!(backgrounds[1], backgrounds[0]);
    assert_eq!(backgrounds[3], backgrounds[0]);
}
