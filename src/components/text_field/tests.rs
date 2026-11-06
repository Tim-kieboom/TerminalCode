use super::{Caret, TextField};
use crossterm::event::KeyCode;

fn field(text: &str) -> TextField {
    TextField::with_text(text)
}

#[test]
fn typing_goes_in_at_the_cursor_and_the_cursor_moves_on() {
    let mut field = field("abd");

    field.move_left();
    field.insert('c');

    assert_eq!(field.text(), "abcd");
    assert_eq!(field.cursor(), 3);
}

#[test]
fn backspace_and_delete_take_one_grapheme_each_side_of_the_cursor() {
    let mut field = field("a\u{301}bc"); // á, b, c
    field.move_caret(Caret::Home);
    field.move_right();
    assert_eq!(
        field.cursor(),
        "a\u{301}".len(),
        "the accent moves with its letter"
    );

    assert!(field.backspace());
    assert_eq!(field.text(), "bc");
    assert!(field.delete());
    assert_eq!(field.text(), "c");
    assert!(field.delete());
    assert!(!field.delete(), "nothing after the cursor");
    assert!(!field.backspace(), "nothing before it");
}

#[test]
fn the_carets_move_by_character_by_word_and_to_the_ends() {
    let mut field = field("let foo_bar = 12;");

    field.move_caret(Caret::WordLeft);
    assert_eq!(&field.text()[field.cursor()..], "12;");
    field.move_caret(Caret::WordLeft);
    assert_eq!(
        &field.text()[field.cursor()..],
        "foo_bar = 12;",
        "_ is part of a word"
    );
    field.move_caret(Caret::WordRight);
    assert_eq!(&field.text()[..field.cursor()], "let foo_bar");
    field.move_caret(Caret::Home);
    assert_eq!(field.cursor(), 0);
    field.move_caret(Caret::Left);
    assert_eq!(field.cursor(), 0, "not past the start");
    field.move_caret(Caret::End);
    field.move_caret(Caret::Right);
    assert_eq!(field.cursor(), field.text().len());
}

#[test]
fn keys_map_to_carets() {
    assert_eq!(Caret::from_key(KeyCode::Left, false), Some(Caret::Left));
    assert_eq!(Caret::from_key(KeyCode::Left, true), Some(Caret::WordLeft));
    assert_eq!(
        Caret::from_key(KeyCode::Right, true),
        Some(Caret::WordRight)
    );
    assert_eq!(Caret::from_key(KeyCode::Home, false), Some(Caret::Home));
    assert_eq!(Caret::from_key(KeyCode::End, true), Some(Caret::End));
    assert_eq!(Caret::from_key(KeyCode::Up, false), None);
}

#[test]
fn a_text_that_fits_is_shown_whole() {
    let field = field("short");

    assert_eq!(field.visible(20), ("short", 5));
}

#[test]
fn a_longer_text_scrolls_to_keep_the_cursor_in_view() {
    let mut field = field("0123456789abcdefghij");

    let (shown, column) = field.visible(8);
    assert_eq!(shown, "cdefghij");
    assert_eq!(column, 8, "the cursor is at the end of what is shown");

    // Moving left inside the window does not slide it.
    field.move_left();
    field.move_left();
    assert_eq!(field.visible(8).0, "cdefghij");

    // Going past its start slides it.
    for _ in 0..8 {
        field.move_left();
    }
    let (shown, column) = field.visible(8);
    assert!(
        shown.starts_with("a") || shown.starts_with("9") || shown.starts_with("b"),
        "{shown}"
    );
    assert!(column <= 8);
    field.move_caret(Caret::Home);
    assert_eq!(field.visible(8).0, "01234567");
}

#[test]
fn a_click_puts_the_cursor_at_the_nearer_edge_of_the_character() {
    let mut field = field("abcdef");
    field.visible(20);

    field.click(2);
    assert_eq!(field.cursor(), 2);
    field.click(100);
    assert_eq!(field.cursor(), 6, "past the text: the end");
}

#[test]
fn a_click_on_a_wide_character_goes_to_its_nearer_edge() {
    let mut field = field("日本語");
    field.visible(20);

    field.click(1);
    assert_eq!(field.cursor(), "日".len(), "the right half goes after it");
    field.click(0);
    assert_eq!(field.cursor(), 0);
}

#[test]
fn set_replace_and_remove_put_the_cursor_where_the_edit_ended() {
    let mut field = field("src/comp");

    field.replace_from(4, "components/");
    assert_eq!(field.text(), "src/components/");
    assert_eq!(field.cursor(), field.text().len());

    field.remove_range(4, field.text().len());
    assert_eq!(field.text(), "src/");
    assert_eq!(field.cursor(), 4);

    field.set("x");
    assert_eq!((field.text(), field.cursor()), ("x", 1));
    field.clear();
    assert!(field.is_empty());
}
