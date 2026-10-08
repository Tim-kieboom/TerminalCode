use proptest::prelude::*;

use crate::buffer::{Buffer, Position};
use crate::components::editor::{Editor, Motion};

/// Text mixing ASCII, punctuation, line breaks, wide, combining and astral
/// characters.
fn text_strategy() -> impl Strategy<Value = String> {
    "[a-z ,_\\n\t\u{e9}\u{301}\u{3053}\u{1F1F3}\u{1F1F1}\u{1F600}]{0,40}"
}

fn head(editor: &Editor) -> Position {
    editor.selections().primary().head()
}

/// Every position the cursor can sit on, in order.
fn all_positions(buffer: &Buffer) -> Vec<Position> {
    let mut positions = Vec::new();
    for line in 0..buffer.len_lines() {
        for column in 0..=buffer.line_len(line).unwrap() {
            positions.push(Position::new(line, column));
        }
    }
    positions
}

proptest! {
    #[test]
    fn walking_right_visits_every_position_once_and_ends_at_the_end(text in text_strategy()) {
        let mut editor = Editor::new(Buffer::from_text(&text));
        let expected = all_positions(editor.buffer());

        let mut visited = vec![head(&editor)];
        loop {
            let before = head(&editor);
            editor.move_cursor(Motion::Right).unwrap();
            if head(&editor) == before {
                break;
            }
            visited.push(head(&editor));
        }

        prop_assert_eq!(visited, expected);
    }

    #[test]
    fn walking_left_from_the_end_reverses_the_walk_right(text in text_strategy()) {
        let mut editor = Editor::new(Buffer::from_text(&text));
        let mut expected = all_positions(editor.buffer());
        expected.reverse();

        editor.move_cursor(Motion::DocumentEnd).unwrap();
        let mut visited = vec![head(&editor)];
        loop {
            let before = head(&editor);
            editor.move_cursor(Motion::Left).unwrap();
            if head(&editor) == before {
                break;
            }
            visited.push(head(&editor));
        }

        prop_assert_eq!(visited, expected);
    }

    #[test]
    fn every_motion_lands_on_a_valid_position(
        text in text_strategy(),
        steps in prop::collection::vec(0usize..10, 0..30),
    ) {
        let motions = [
            Motion::Left, Motion::Right, Motion::Up, Motion::Down,
            Motion::WordLeft, Motion::WordRight,
            Motion::LineStart, Motion::LineEnd,
            Motion::DocumentStart, Motion::DocumentEnd,
        ];
        let mut editor = Editor::new(Buffer::from_text(&text));

        for step in steps {
            editor.move_cursor(motions[step]).unwrap();

            let position = head(&editor);
            prop_assert!(editor.buffer().position_to_byte(position).is_ok(), "{position:?}");
        }
    }

    #[test]
    fn word_motions_make_progress_in_their_direction(text in text_strategy()) {
        let mut editor = Editor::new(Buffer::from_text(&text));
        loop {
            let before = head(&editor);
            editor.move_cursor(Motion::WordRight).unwrap();
            prop_assert!(head(&editor) >= before);
            if head(&editor) == before {
                break;
            }
        }
        prop_assert_eq!(
            editor.buffer().position_to_byte(head(&editor)).unwrap(),
            editor.buffer().len_bytes()
        );

        loop {
            let before = head(&editor);
            editor.move_cursor(Motion::WordLeft).unwrap();
            prop_assert!(head(&editor) <= before);
            if head(&editor) == before {
                break;
            }
        }
        prop_assert_eq!(head(&editor), Position::new(0, 0));
    }

    #[test]
    fn typing_then_undoing_restores_text_and_cursor(
        text in text_strategy(),
        typed in "[a-z \u{e9}\u{1F600}]{1,8}",
        moves in prop::collection::vec(0usize..4, 0..6),
    ) {
        let motions = [Motion::Left, Motion::Right, Motion::Up, Motion::Down];
        let mut editor = Editor::new(Buffer::from_text(&text));
        for step in moves {
            editor.move_cursor(motions[step]).unwrap();
        }
        let before = head(&editor);

        for c in typed.chars() {
            editor.insert_text(&c.to_string()).unwrap();
        }
        editor.undo().unwrap();

        prop_assert_eq!(editor.buffer().text(), text);
        prop_assert_eq!(head(&editor), before);
    }
}
