use proptest::prelude::*;

use crate::buffer::Buffer;
use crate::buffer::edit::Edit;
use crate::buffer::position::Position;
use crate::buffer::selection::Selections;

/// Text mixing ASCII, line breaks, wide, combining and astral characters.
fn text_strategy() -> impl Strategy<Value = String> {
    "[a-z \\n\u{e9}\u{301}\u{3053}\u{1F1F3}\u{1F1F1}\r]{0,40}"
}

/// Maps an arbitrary number onto a byte offset on a char boundary of `text`.
fn boundary(text: &str, seed: usize) -> usize {
    let offsets: Vec<usize> = text
        .char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(text.len()))
        .collect();
    offsets[seed % offsets.len()]
}

proptest! {
    #[test]
    fn applying_an_edit_then_its_inverse_restores_the_text(
        text in text_strategy(),
        insert in text_strategy(),
        a in any::<usize>(),
        b in any::<usize>(),
    ) {
        let (start, end) = (boundary(&text, a), boundary(&text, b));
        let range = start.min(end)..start.max(end);
        let mut buffer = Buffer::from_text(&text);

        let mut transaction = buffer.begin_transaction(Selections::default());
        transaction.apply(&Edit::new(range, insert.as_str())).unwrap();
        transaction.commit(Selections::default());
        buffer.undo().unwrap();

        prop_assert_eq!(buffer.text(), text);
    }

    #[test]
    fn undo_then_redo_reproduces_the_edited_text(
        text in text_strategy(),
        insert in text_strategy(),
        a in any::<usize>(),
    ) {
        let at = boundary(&text, a);
        let mut buffer = Buffer::from_text(&text);

        let mut transaction = buffer.begin_transaction(Selections::default());
        transaction.apply(&Edit::insert(at, insert.as_str())).unwrap();
        transaction.commit(Selections::default());
        let edited = buffer.text();
        buffer.undo().unwrap();
        buffer.redo().unwrap();

        prop_assert_eq!(buffer.text(), edited);
    }

    #[test]
    fn every_valid_position_round_trips_through_bytes(text in text_strategy()) {
        let buffer = Buffer::from_text(&text);

        for line in 0..buffer.len_lines() {
            let mut column = 0;
            while let Ok(byte) = buffer.position_to_byte(Position::new(line, column)) {
                prop_assert_eq!(
                    buffer.byte_to_position(byte),
                    Ok(Position::new(line, column))
                );
                column += 1;
            }
        }
    }
}
