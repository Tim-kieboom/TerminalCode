use crate::buffer::{Buffer, Position, Selection};
use crate::editor::{Editor, Motion};

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
