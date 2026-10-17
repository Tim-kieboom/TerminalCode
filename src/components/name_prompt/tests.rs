use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::*;

fn prompt(text: &str) -> NamePrompt {
    let mut prompt = NamePrompt::open_file(PathBuf::from("/p"));
    for c in text.chars() {
        prompt.insert(c);
    }
    prompt
}

fn key(code: KeyCode, modifiers: KeyModifiers) -> Option<Command> {
    Command::from_key(KeyEvent::new(code, modifiers))
}

/// Draws the prompt on a screen of `width` columns.
fn draw(prompt: &mut NamePrompt, width: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, 12)).unwrap();
    prompt.place(Rect::new(0, 0, width, 12));
    let prompt = &*prompt;
    let theme = Theme::default();
    terminal.draw(|frame| prompt.render(frame, &theme)).unwrap();
    terminal
}

#[test]
fn typing_goes_in_at_the_cursor() {
    let mut prompt = prompt("abd");

    prompt.move_left();
    prompt.insert('c');

    assert_eq!(prompt.name(), "abcd");
    assert_eq!(prompt.cursor(), 3);
}

#[test]
fn the_cursor_stays_inside_the_text() {
    let mut prompt = prompt("ab");

    prompt.move_right();
    assert_eq!(prompt.cursor(), 2);
    prompt.move_home();
    prompt.move_left();
    assert_eq!(prompt.cursor(), 0);
    prompt.move_end();
    assert_eq!(prompt.cursor(), 2);
}

#[test]
fn backspace_and_delete_work_at_both_ends_and_in_the_middle() {
    let mut prompt = prompt("abc");

    prompt.delete();
    assert_eq!(prompt.name(), "abc", "nothing after the cursor");
    prompt.move_home();
    prompt.backspace();
    assert_eq!(prompt.name(), "abc", "nothing before the cursor");

    prompt.move_right();
    prompt.delete();
    assert_eq!(prompt.name(), "ac");
    prompt.backspace();
    assert_eq!(prompt.name(), "c");
    assert_eq!(prompt.cursor(), 0);
}

#[test]
fn the_cursor_moves_by_grapheme_not_by_character() {
    // A flag is two characters, an e with an accent is two, a CJK character is wide.
    let mut prompt = prompt("a🇳🇱e\u{301}日b");

    prompt.move_left();
    prompt.move_left();
    prompt.move_left();
    prompt.move_left();
    assert_eq!(prompt.cursor(), "a".len());
    prompt.move_right();
    assert_eq!(prompt.cursor(), "a🇳🇱".len());

    prompt.move_right();
    prompt.backspace();
    assert_eq!(prompt.name(), "a🇳🇱日b", "the accent went with its letter");
    prompt.move_left();
    prompt.delete();
    assert_eq!(prompt.name(), "a日b", "the whole flag went");
}

#[test]
fn a_wide_character_is_deleted_whole() {
    let mut prompt = prompt("日本");

    prompt.backspace();

    assert_eq!(prompt.name(), "日");
}

#[test]
fn word_moves_go_over_a_word_at_a_time_and_stop_at_dots() {
    let mut prompt = prompt("my_file.name.rs");

    prompt.move_word_left();
    assert_eq!(prompt.cursor(), "my_file.name.".len());
    prompt.move_word_left();
    assert_eq!(prompt.cursor(), "my_file.".len());
    prompt.move_word_left();
    assert_eq!(prompt.cursor(), 0);
    prompt.move_word_left();
    assert_eq!(prompt.cursor(), 0);

    prompt.move_word_right();
    assert_eq!(prompt.cursor(), "my_file".len());
    prompt.move_word_right();
    assert_eq!(prompt.cursor(), "my_file.name".len());
    prompt.move_word_right();
    prompt.move_word_right();
    assert_eq!(prompt.cursor(), prompt.name().len());
}

#[test]
fn clearing_puts_the_cursor_at_the_start() {
    let mut prompt = prompt("abc");

    prompt.clear();
    prompt.insert('x');

    assert_eq!(prompt.name(), "x");
    assert_eq!(prompt.cursor(), 1);
}

#[test]
fn a_rename_starts_with_the_cursor_after_the_old_name() {
    let prompt = NamePrompt::rename(PathBuf::from("/p/a.txt"));

    assert_eq!(prompt.cursor(), "a.txt".len());
}

#[test]
fn the_keys_map_to_the_commands() {
    let none = KeyModifiers::NONE;
    let ctrl = KeyModifiers::CONTROL;
    assert_eq!(key(KeyCode::Left, none), Some(Command::Left));
    assert_eq!(key(KeyCode::Right, none), Some(Command::Right));
    assert_eq!(key(KeyCode::Home, none), Some(Command::Home));
    assert_eq!(key(KeyCode::End, none), Some(Command::End));
    assert_eq!(key(KeyCode::Delete, none), Some(Command::Delete));
    assert_eq!(key(KeyCode::Left, ctrl), Some(Command::WordLeft));
    assert_eq!(key(KeyCode::Right, ctrl), Some(Command::WordRight));
}

#[test]
fn the_text_cursor_is_drawn_where_typing_goes() {
    let mut prompt = prompt("abcd");
    prompt.move_left();
    prompt.move_left();

    let mut terminal = draw(&mut prompt, 80);

    // The box is centered; the text starts after the border and "> ".
    let start = (80 - WIDTH) / 2 + 1 + PROMPT.width() as u16;
    assert_eq!(terminal.get_cursor_position().unwrap().x, start + 2);
}

#[test]
fn a_click_puts_the_cursor_between_the_characters_it_is_on() {
    let mut prompt = prompt("abcd");
    draw(&mut prompt, 80);
    let start = (80 - WIDTH) / 2 + 1 + PROMPT.width() as u16;
    let row = prompt.layout.text_y;

    assert!(prompt.click(start + 2, row));
    assert_eq!(prompt.cursor(), 2);
    assert!(prompt.click(start, row));
    assert_eq!(prompt.cursor(), 0);
    assert!(prompt.click(start + 30, row), "past the end is the end");
    assert_eq!(prompt.cursor(), 4);
}

#[test]
fn a_click_outside_the_text_does_nothing() {
    let mut prompt = prompt("abcd");
    draw(&mut prompt, 80);
    let drawn = prompt.layout;

    assert!(!prompt.click(drawn.text_x, drawn.text_y + 1));
    assert!(!prompt.click(drawn.text_x - 1, drawn.text_y));
    assert_eq!(prompt.cursor(), 4);
}

#[test]
fn a_click_on_the_halves_of_a_wide_character_goes_before_or_after_it() {
    let mut prompt = prompt("日本");
    draw(&mut prompt, 80);
    let drawn = prompt.layout;

    prompt.click(drawn.text_x + 2, drawn.text_y);
    assert_eq!(prompt.cursor(), "日".len(), "the left half of the second");
    prompt.click(drawn.text_x + 1, drawn.text_y);
    assert_eq!(prompt.cursor(), "日".len(), "the right half of the first");
    prompt.click(drawn.text_x, drawn.text_y);
    assert_eq!(prompt.cursor(), 0);
}

#[test]
fn a_name_longer_than_the_box_scrolls_to_keep_the_cursor_in_view() {
    let long = "x".repeat(100);
    let mut prompt = prompt(&long);

    let mut at_end = draw(&mut prompt, 80);
    let inner_right = (80 + WIDTH) / 2 - 1;
    assert!(at_end.get_cursor_position().unwrap().x <= inner_right);

    prompt.move_home();
    let mut at_start = draw(&mut prompt, 80);
    let text_start = (80 - WIDTH) / 2 + 1 + PROMPT.width() as u16;
    assert_eq!(at_start.get_cursor_position().unwrap().x, text_start);
    assert_eq!(prompt.scroll, 0);
}

#[test]
fn a_scroll_offset_from_before_an_edit_does_not_cut_a_character_in_two() {
    // Long, with characters of two bytes, so that an offset kept from an earlier
    // frame can fall inside one once the text before it changes.
    let mut prompt = prompt(&"\u{e9}".repeat(100));
    draw(&mut prompt, 80);
    assert!(prompt.scroll > 0, "the name is scrolled");

    prompt.move_home();
    prompt.insert('x');
    prompt.move_end();
    let mut terminal = draw(&mut prompt, 80);

    assert_eq!(
        prompt.scroll % 2,
        1,
        "after the `x` the characters start at odd bytes"
    );
    assert!(terminal.get_cursor_position().is_ok());
    prompt.click(prompt.layout.text_x + 3, prompt.layout.text_y);
    assert!(prompt.name.is_char_boundary(prompt.cursor()));
}

#[test]
fn a_click_before_the_next_draw_after_an_edit_stays_on_a_character_boundary() {
    let mut prompt = prompt(&"\u{e9}".repeat(100));
    draw(&mut prompt, 80);

    prompt.move_home();
    prompt.insert('x');
    prompt.move_end();
    prompt.click(prompt.layout.text_x + 3, prompt.layout.text_y);

    assert!(prompt.name.is_char_boundary(prompt.cursor()));
}
