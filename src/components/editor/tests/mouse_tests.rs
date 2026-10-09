use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

use crate::app::state::AppState;
use crate::buffer::{Buffer, Position};
use crate::components::editor::{Editor, Motion};
use crate::event::mouse::Clicks;

/// Text starts at screen cell (5, 1) and is 40 x 10 cells.
const AREA: Rect = Rect::new(5, 1, 40, 10);

fn editor(text: &str) -> Editor {
    let mut editor = Editor::new(Buffer::from_text(text));
    editor.set_viewport(AREA);
    editor
}

fn head(editor: &Editor) -> Position {
    editor.selections().primary().head()
}

/// Screen cell of display column `col` on visible row `row`.
fn cell(col: u16, row: u16) -> (u16, u16) {
    (AREA.x + col, AREA.y + row)
}

fn click(editor: &mut Editor, col: u16, row: u16) {
    let (x, y) = cell(col, row);
    editor.mouse_press(x, y, false, Clicks::Single).unwrap();
}

fn multi_click(editor: &mut Editor, col: u16, row: u16, clicks: Clicks) {
    let (x, y) = cell(col, row);
    editor.mouse_press(x, y, false, clicks).unwrap();
}

fn selected(editor: &Editor) -> (Position, Position) {
    let selection = editor.selections().primary();
    (selection.start(), selection.end())
}

#[test]
fn a_click_places_the_cursor() {
    let mut editor = editor("hello\nworld");

    click(&mut editor, 3, 1);

    assert_eq!(head(&editor), Position::new(1, 3));
    assert!(editor.selections().primary().is_empty());
}

#[test]
fn a_click_outside_the_text_area_is_ignored() {
    let mut editor = editor("hello");
    click(&mut editor, 2, 0);

    // In the gutter, left of the text.
    editor.mouse_press(2, 1, false, Clicks::Single).unwrap();
    // Below the text area.
    editor.mouse_press(10, 20, false, Clicks::Single).unwrap();

    assert_eq!(head(&editor), Position::new(0, 2));
}

#[test]
fn a_click_right_of_a_line_goes_to_its_end() {
    let mut editor = editor("ab\ncdef");

    click(&mut editor, 30, 0);

    assert_eq!(head(&editor), Position::new(0, 2));
}

#[test]
fn a_click_below_the_last_line_goes_to_the_end_of_the_document() {
    let mut editor = editor("ab\ncdef");

    click(&mut editor, 1, 8);

    assert_eq!(head(&editor), Position::new(1, 4));
}

#[test]
fn clicks_account_for_vertical_and_horizontal_scroll() {
    let lines: Vec<String> = (0..50)
        .map(|n| format!("{n:02}-{}", "x".repeat(60)))
        .collect();
    let mut editor = editor(&lines.join("\n"));
    editor.scroll_lines(2);
    editor.scroll_columns(1);
    assert_eq!((editor.scroll().top, editor.scroll().left), (6, 6));

    click(&mut editor, 4, 2);

    assert_eq!(head(&editor), Position::new(8, 10));
}

#[test]
fn clicks_account_for_tabs_and_wide_characters() {
    let mut editor = editor("\ta\u{3053}b");

    // The tab covers display columns 0..4; `a` is column 4.
    click(&mut editor, 4, 0);
    assert_eq!(head(&editor), Position::new(0, 1));

    // `a` is column 4, the wide char 5..7; the right half of it is after it.
    click(&mut editor, 6, 0);
    assert_eq!(head(&editor), Position::new(0, 3));
}

#[test]
fn a_click_resets_a_remembered_desired_column() {
    let mut editor = editor("abcdef\nab\nabcdef");
    click(&mut editor, 5, 0);
    editor.move_cursor(Motion::Down).unwrap();

    click(&mut editor, 1, 1);
    editor.move_cursor(Motion::Down).unwrap();

    assert_eq!(head(&editor), Position::new(2, 1));
}

#[test]
fn shift_click_extends_from_the_existing_anchor() {
    let mut editor = editor("abcdef");
    click(&mut editor, 1, 0);
    let (x, y) = cell(4, 0);

    editor.mouse_press(x, y, true, Clicks::Single).unwrap();

    assert_eq!(
        selected(&editor),
        (Position::new(0, 1), Position::new(0, 4))
    );
}

#[test]
fn dragging_selects_from_the_press_to_the_pointer() {
    let mut editor = editor("hello\nworld");
    click(&mut editor, 1, 0);
    let (x, y) = cell(3, 1);

    editor.mouse_drag(x, y).unwrap();

    assert_eq!(
        selected(&editor),
        (Position::new(0, 1), Position::new(1, 3))
    );
}

#[test]
fn dragging_backwards_keeps_the_press_as_the_anchor() {
    let mut editor = editor("hello\nworld");
    click(&mut editor, 3, 1);
    let (x, y) = cell(1, 0);

    editor.mouse_drag(x, y).unwrap();

    let selection = *editor.selections().primary();
    assert_eq!(selection.anchor(), Position::new(1, 3));
    assert_eq!(selection.head(), Position::new(0, 1));
}

#[test]
fn dragging_outside_the_text_selects_to_its_edge() {
    let mut editor = editor("one\ntwo\nthree");
    click(&mut editor, 1, 1);

    // Far below and right of the text area.
    editor.mouse_drag(200, 50).unwrap();
    assert_eq!(selected(&editor).1, Position::new(2, 5));

    // Above the text area.
    editor.mouse_drag(0, 0).unwrap();
    assert_eq!(selected(&editor).0, Position::new(0, 0));
}

#[test]
fn dragging_without_a_press_does_nothing() {
    let mut editor = editor("hello");
    let (x, y) = cell(3, 0);

    editor.mouse_drag(x, y).unwrap();

    assert_eq!(head(&editor), Position::new(0, 0));
}

#[test]
fn releasing_the_button_ends_the_drag() {
    let mut editor = editor("hello");
    click(&mut editor, 1, 0);
    editor.mouse_release();
    let (x, y) = cell(4, 0);

    editor.mouse_drag(x, y).unwrap();

    assert!(editor.selections().primary().is_empty());
}

#[test]
fn double_click_selects_the_word_under_the_pointer() {
    let mut editor = editor("let foo_bar = 1;");

    multi_click(&mut editor, 6, 0, Clicks::Double);

    assert_eq!(
        selected(&editor),
        (Position::new(0, 4), Position::new(0, 11))
    );
}

#[test]
fn double_click_on_punctuation_or_spaces_selects_that_run() {
    let mut editor = editor("a ,, b");

    multi_click(&mut editor, 3, 0, Clicks::Double);
    assert_eq!(
        selected(&editor),
        (Position::new(0, 2), Position::new(0, 4))
    );

    multi_click(&mut editor, 1, 0, Clicks::Double);
    assert_eq!(
        selected(&editor),
        (Position::new(0, 1), Position::new(0, 2))
    );
}

#[test]
fn double_click_on_an_empty_line_selects_nothing() {
    let mut editor = editor("a\n\nb");

    multi_click(&mut editor, 0, 1, Clicks::Double);

    assert!(editor.selections().primary().is_empty());
    assert_eq!(head(&editor), Position::new(1, 0));
}

#[test]
fn triple_click_selects_the_line_with_its_break() {
    let mut editor = editor("one\ntwo\nthree");

    multi_click(&mut editor, 1, 1, Clicks::Triple);

    assert_eq!(
        selected(&editor),
        (Position::new(1, 0), Position::new(2, 0))
    );
}

#[test]
fn triple_click_on_the_last_line_stops_at_its_end() {
    let mut editor = editor("one\ntwo");

    multi_click(&mut editor, 1, 1, Clicks::Triple);

    assert_eq!(
        selected(&editor),
        (Position::new(1, 0), Position::new(1, 3))
    );
}

#[test]
fn dragging_after_a_double_click_extends_from_the_start_of_the_word() {
    let mut editor = editor("foo bar baz");
    multi_click(&mut editor, 5, 0, Clicks::Double);
    let (x, y) = cell(9, 0);

    editor.mouse_drag(x, y).unwrap();

    assert_eq!(
        selected(&editor),
        (Position::new(0, 4), Position::new(0, 9))
    );
}

#[test]
fn the_wheel_scrolls_by_three_lines_and_stops_at_the_ends() {
    let text = (0..20)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let mut editor = editor(&text);

    editor.scroll_lines(1);
    assert_eq!(editor.scroll().top, 3);

    editor.scroll_lines(-5);
    assert_eq!(editor.scroll().top, 0);

    editor.scroll_lines(100);
    assert_eq!(editor.scroll().top, 19);
}

#[test]
fn the_wheel_does_not_move_the_cursor() {
    let text = (0..50)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let mut editor = editor(&text);

    editor.scroll_lines(5);

    assert_eq!(head(&editor), Position::new(0, 0));
}

#[test]
fn sideways_scrolling_stops_at_the_end_of_the_longest_visible_line() {
    let mut editor = editor(&"x".repeat(20));

    for _ in 0..10 {
        editor.scroll_columns(1);
    }

    assert_eq!(editor.scroll().left, 19);
    editor.scroll_columns(-100);
    assert_eq!(editor.scroll().left, 0);
}

#[test]
fn the_view_only_follows_the_cursor_when_something_changed() {
    let mut editor = editor("abc\ndef");
    assert!(editor.take_view_change(), "first look always counts");
    assert!(!editor.take_view_change());

    editor.scroll_lines(1);
    assert!(!editor.take_view_change(), "scrolling must not count");

    editor.move_cursor(Motion::Down).unwrap();
    assert!(editor.take_view_change());

    editor.insert_text("x").unwrap();
    assert!(editor.take_view_change());

    editor.set_viewport(Rect::new(5, 1, 30, 8));
    assert!(editor.take_view_change(), "a resize counts");
}

#[test]
fn a_frame_after_wheel_scrolling_does_not_snap_back_to_the_cursor() {
    let text = (0..200)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut state = AppState::new(Editor::new(Buffer::from_text(&text)));
    let mut terminal = Terminal::new(TestBackend::new(100, 40)).unwrap();
    terminal
        .draw(|frame| crate::components::prepare_and_render(frame, &mut state))
        .unwrap();

    state.edit(|e| e.scroll_lines(10));
    terminal
        .draw(|frame| crate::components::prepare_and_render(frame, &mut state))
        .unwrap();

    assert_eq!(state.editor().scroll().top, 30);
    let screen = terminal.backend().to_string();
    assert!(screen.contains("line 30"), "screen:\n{screen}");
}
