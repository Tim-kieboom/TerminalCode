use crate::buffer::{Buffer, Position, Selection};
use crate::components::editor::{Editor, Motion};

const FLAG: &str = "\u{1F1F3}\u{1F1F1}";

fn editor(text: &str) -> Editor {
    Editor::new(Buffer::from_text(text))
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

#[test]
fn right_steps_over_whole_graphemes() {
    let mut editor = editor(&format!("a{FLAG}b"));

    editor.move_cursor(Motion::Right).unwrap();
    editor.move_cursor(Motion::Right).unwrap();

    assert_eq!(head(&editor), Position::new(0, 2));
}

#[test]
fn right_wraps_to_the_next_line_and_stops_at_the_end() {
    let mut editor = editor("ab\ncd");
    goto(&mut editor, 0, 2);

    editor.move_cursor(Motion::Right).unwrap();
    assert_eq!(head(&editor), Position::new(1, 0));

    editor.move_cursor(Motion::DocumentEnd).unwrap();
    editor.move_cursor(Motion::Right).unwrap();
    assert_eq!(head(&editor), Position::new(1, 2));
}

#[test]
fn left_wraps_to_the_previous_line_end_and_stops_at_the_start() {
    let mut editor = editor("ab\ncd");
    goto(&mut editor, 1, 0);

    editor.move_cursor(Motion::Left).unwrap();
    assert_eq!(head(&editor), Position::new(0, 2));

    editor.move_cursor(Motion::DocumentStart).unwrap();
    editor.move_cursor(Motion::Left).unwrap();
    assert_eq!(head(&editor), Position::new(0, 0));
}

#[test]
fn vertical_motion_remembers_the_desired_column() {
    let mut editor = editor("abcdef\nab\nabcdef");
    goto(&mut editor, 0, 5);

    editor.move_cursor(Motion::Down).unwrap();
    assert_eq!(head(&editor), Position::new(1, 2));

    editor.move_cursor(Motion::Down).unwrap();
    assert_eq!(head(&editor), Position::new(2, 5));
}

#[test]
fn horizontal_motion_forgets_the_desired_column() {
    let mut editor = editor("abcdef\nab\nabcdef");
    goto(&mut editor, 0, 5);
    editor.move_cursor(Motion::Down).unwrap();

    editor.move_cursor(Motion::Left).unwrap();
    editor.move_cursor(Motion::Down).unwrap();

    assert_eq!(head(&editor), Position::new(2, 1));
}

#[test]
fn up_on_the_first_line_goes_to_its_start_and_down_on_the_last_to_its_end() {
    let mut editor = editor("abc\ndef");
    goto(&mut editor, 0, 2);

    editor.move_cursor(Motion::Up).unwrap();
    assert_eq!(head(&editor), Position::new(0, 0));

    editor.move_cursor(Motion::Down).unwrap();
    editor.move_cursor(Motion::Down).unwrap();
    assert_eq!(head(&editor), Position::new(1, 3));
}

#[test]
fn line_and_document_motions() {
    let mut editor = editor("abc\ndef\nghi");
    goto(&mut editor, 1, 1);

    editor.move_cursor(Motion::LineEnd).unwrap();
    assert_eq!(head(&editor), Position::new(1, 3));

    editor.move_cursor(Motion::LineStart).unwrap();
    assert_eq!(head(&editor), Position::new(1, 0));

    editor.move_cursor(Motion::DocumentEnd).unwrap();
    assert_eq!(head(&editor), Position::new(2, 3));

    editor.move_cursor(Motion::DocumentStart).unwrap();
    assert_eq!(head(&editor), Position::new(0, 0));
}

#[test]
fn select_extends_from_the_anchor_in_either_direction() {
    let mut editor = editor("abcdef");
    goto(&mut editor, 0, 3);

    editor.select(Motion::Right).unwrap();
    editor.select(Motion::Right).unwrap();
    let selection = *editor.selections().primary();
    assert_eq!(selection.anchor(), Position::new(0, 3));
    assert_eq!(selection.head(), Position::new(0, 5));

    for _ in 0..4 {
        editor.select(Motion::Left).unwrap();
    }
    let selection = *editor.selections().primary();
    assert_eq!(selection.anchor(), Position::new(0, 3));
    assert_eq!(selection.head(), Position::new(0, 1));
    assert_eq!(selection.start(), Position::new(0, 1));
}

#[test]
fn left_and_right_collapse_a_selection_to_its_ends() {
    let mut editor = editor("abcdef");
    goto(&mut editor, 0, 1);
    editor.select(Motion::Right).unwrap();
    editor.select(Motion::Right).unwrap();
    let selected = *editor.selections().primary();

    editor.move_cursor(Motion::Left).unwrap();
    assert_eq!(
        *editor.selections().primary(),
        Selection::cursor(selected.start())
    );

    editor.select(Motion::Right).unwrap();
    editor.select(Motion::Right).unwrap();
    editor.move_cursor(Motion::Right).unwrap();
    assert_eq!(head(&editor), selected.end());
    assert!(editor.selections().primary().is_empty());
}

#[test]
fn moving_in_an_empty_buffer_stays_at_the_origin() {
    let mut editor = Editor::default();

    for motion in [Motion::Left, Motion::Right, Motion::Up, Motion::Down] {
        editor.move_cursor(motion).unwrap();
        assert_eq!(head(&editor), Position::new(0, 0));
    }
}

fn word_stops_right(text: &str) -> Vec<usize> {
    let mut editor = editor(text);
    let mut stops = Vec::new();
    loop {
        let before = head(&editor);
        editor.move_cursor(Motion::WordRight).unwrap();
        if head(&editor) == before {
            return stops;
        }
        stops.push(head(&editor).column);
    }
}

#[test]
fn word_right_stops_at_the_end_of_each_word_and_punctuation_run() {
    //            0123456789012345
    let stops = word_stops_right("foo bar_baz, (qux)");

    assert_eq!(stops, [3, 11, 12, 14, 17, 18]);
}

#[test]
fn word_left_stops_at_the_start_of_each_word() {
    let mut editor = editor("foo  bar_baz");
    editor.move_cursor(Motion::LineEnd).unwrap();

    editor.move_cursor(Motion::WordLeft).unwrap();
    assert_eq!(head(&editor), Position::new(0, 5));

    editor.move_cursor(Motion::WordLeft).unwrap();
    assert_eq!(head(&editor), Position::new(0, 0));
}

#[test]
fn word_motions_cross_line_boundaries_one_step_at_a_time() {
    let mut editor = editor("ab\ncd");
    goto(&mut editor, 0, 2);

    editor.move_cursor(Motion::WordRight).unwrap();
    assert_eq!(head(&editor), Position::new(1, 0));

    editor.move_cursor(Motion::WordLeft).unwrap();
    assert_eq!(head(&editor), Position::new(0, 2));
}

#[test]
fn trailing_spaces_stop_at_the_line_end_not_the_next_line() {
    let mut editor = editor("ab   \ncd");
    goto(&mut editor, 0, 2);

    editor.move_cursor(Motion::WordRight).unwrap();

    assert_eq!(head(&editor), Position::new(0, 5));
}

#[test]
fn unicode_letters_and_graphemes_count_as_word_characters() {
    let mut editor = editor(&format!("caf\u{e9} \u{3053}\u{3093} {FLAG}x"));

    editor.move_cursor(Motion::WordRight).unwrap();
    assert_eq!(head(&editor), Position::new(0, 4));

    editor.move_cursor(Motion::WordRight).unwrap();
    assert_eq!(head(&editor), Position::new(0, 7));
}

#[test]
fn word_motions_in_an_empty_buffer_stay_put() {
    let mut editor = Editor::default();

    editor.move_cursor(Motion::WordRight).unwrap();
    editor.move_cursor(Motion::WordLeft).unwrap();

    assert_eq!(head(&editor), Position::new(0, 0));
}

#[test]
fn select_word_extends_the_selection() {
    let mut editor = editor("hello world");

    editor.select(Motion::WordRight).unwrap();
    editor.select(Motion::WordRight).unwrap();

    let selection = *editor.selections().primary();
    assert_eq!(selection.start(), Position::new(0, 0));
    assert_eq!(selection.end(), Position::new(0, 11));
}

#[test]
fn select_all_covers_the_whole_document() {
    let mut editor = editor("ab\ncd\nef");

    editor.select_all().unwrap();

    let selection = *editor.selections().primary();
    assert_eq!(selection.start(), Position::new(0, 0));
    assert_eq!(selection.end(), Position::new(2, 2));
}

#[test]
fn select_all_in_an_empty_buffer_is_an_empty_selection() {
    let mut editor = Editor::default();

    editor.select_all().unwrap();

    assert!(editor.selections().primary().is_empty());
}

fn numbered(lines: usize) -> Editor {
    let text = (0..lines)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    editor(&text)
}

/// Pretends a viewport of `height` rows was drawn.
fn viewport(editor: &mut Editor, height: usize) {
    editor.scroll_to_show(0, 0, height, 80);
}

#[test]
fn page_down_moves_a_screenful_minus_one() {
    let mut editor = numbered(100);
    viewport(&mut editor, 21);

    editor.move_cursor(Motion::PageDown).unwrap();

    assert_eq!(head(&editor).line, 20);
}

#[test]
fn page_up_goes_back_up_and_stops_at_the_top() {
    let mut editor = numbered(100);
    viewport(&mut editor, 21);
    editor.move_cursor(Motion::PageDown).unwrap();
    editor.move_cursor(Motion::PageDown).unwrap();

    editor.move_cursor(Motion::PageUp).unwrap();
    assert_eq!(head(&editor).line, 20);

    editor.move_cursor(Motion::PageUp).unwrap();
    assert_eq!(head(&editor).line, 0);

    editor.move_cursor(Motion::PageUp).unwrap();
    assert_eq!(head(&editor), Position::new(0, 0));
}

#[test]
fn page_down_stops_at_the_last_line_then_the_document_end() {
    let mut editor = numbered(30);
    viewport(&mut editor, 21);

    editor.move_cursor(Motion::PageDown).unwrap();
    editor.move_cursor(Motion::PageDown).unwrap();
    assert_eq!(head(&editor).line, 29);

    editor.move_cursor(Motion::PageDown).unwrap();
    assert_eq!(head(&editor), Position::new(29, "line 29".len()));
}

#[test]
fn page_motion_keeps_the_desired_column() {
    let text = format!("{}\nab\n{}", "x".repeat(10), "y".repeat(10));
    let mut editor = editor(&text);
    viewport(&mut editor, 3);
    goto(&mut editor, 0, 8);

    editor.move_cursor(Motion::PageDown).unwrap();

    assert_eq!(head(&editor), Position::new(2, 8));
}

#[test]
fn page_motion_scrolls_the_viewport_with_the_cursor() {
    let mut editor = numbered(100);
    viewport(&mut editor, 21);

    editor.move_cursor(Motion::PageDown).unwrap();

    assert_eq!(editor.scroll().top, 20);
}

#[test]
fn page_up_scrolls_back_the_same_amount() {
    let mut editor = numbered(100);
    viewport(&mut editor, 21);
    editor.move_cursor(Motion::PageDown).unwrap();
    editor.move_cursor(Motion::PageDown).unwrap();
    assert_eq!(editor.scroll().top, 40);

    editor.move_cursor(Motion::PageUp).unwrap();

    assert_eq!(editor.scroll().top, 20);
}

#[test]
fn shift_page_down_selects_a_page() {
    let mut editor = numbered(100);
    viewport(&mut editor, 11);

    editor.select(Motion::PageDown).unwrap();

    let selection = *editor.selections().primary();
    assert_eq!(selection.start(), Position::new(0, 0));
    assert_eq!(selection.end().line, 10);
}

#[test]
fn page_motion_before_anything_was_drawn_still_moves() {
    let mut editor = numbered(10);

    editor.move_cursor(Motion::PageDown).unwrap();

    assert_eq!(head(&editor).line, 1);
}
