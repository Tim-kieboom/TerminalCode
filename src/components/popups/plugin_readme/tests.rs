use crossterm::event::KeyModifiers;

use super::*;

#[test]
fn a_line_wraps_at_spaces_and_a_long_word_is_cut() {
    assert_eq!(wrap_indented("aa bb cc", 5), ["aa bb", "cc"]);
    assert_eq!(wrap_indented("abcdefgh", 3), ["abc", "def", "gh"]);
    assert_eq!(wrap_indented("", 5), [""]);
    assert_eq!(wrap_indented("short", 20), ["short"]);
}

#[test]
fn wide_characters_count_two_columns() {
    assert_eq!(wrap_indented("日本語", 4), ["日本", "語"]);
}

#[test]
fn scrolling_stays_between_the_top_and_the_end() {
    let mut page = Readme::new("x", None, "X".into(), vec![], Body::Loading);
    page.max_scroll.set(5);
    page.page.set(3);
    page.apply(Command::Scroll(-1));
    assert_eq!(page.scroll, 0);
    page.apply(Command::Page(1));
    assert_eq!(page.scroll, 3);
    page.apply(Command::Page(1));
    assert_eq!(page.scroll, 5);
    page.apply(Command::Top);
    assert_eq!(page.scroll, 0);
    page.apply(Command::Bottom);
    assert_eq!(page.scroll, 5);
}

#[test]
fn keys_map_to_commands() {
    let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
    assert_eq!(Command::from_key(key(KeyCode::Esc)), Some(Command::Close));
    assert_eq!(
        Command::from_key(key(KeyCode::Char('d'))),
        Some(Command::Close)
    );
    assert_eq!(
        Command::from_key(key(KeyCode::Down)),
        Some(Command::Scroll(1))
    );
    assert_eq!(Command::from_key(key(KeyCode::Char('z'))), None);
}
