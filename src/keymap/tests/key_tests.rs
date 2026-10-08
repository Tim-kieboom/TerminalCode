use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::keymap::key::{KeyChord, KeyParseError};

const CTRL: KeyModifiers = KeyModifiers::CONTROL;
const SHIFT: KeyModifiers = KeyModifiers::SHIFT;

fn chord(source: &str) -> KeyChord {
    source.parse().unwrap()
}

#[test]
fn ctrl_letter_parses_to_lowercase_char() {
    let parsed = chord("ctrl+s");

    assert_eq!(parsed.code(), KeyCode::Char('s'));
    assert_eq!(parsed.modifiers(), CTRL);
}

#[test]
fn parsing_ignores_case_of_names_and_modifier_order() {
    assert_eq!(chord("Ctrl+Shift+P"), chord("shift+ctrl+p"));
    assert_eq!(chord("ENTER"), chord("enter"));
}

#[test]
fn shortcut_with_uppercase_letter_is_lowercase_plus_shift() {
    let event = KeyEvent::new(KeyCode::Char('P'), CTRL | SHIFT);

    let from_terminal = KeyChord::from_event(event).unwrap();

    assert_eq!(from_terminal, chord("ctrl+shift+p"));
    assert_eq!(from_terminal.code(), KeyCode::Char('p'));
}

#[test]
fn plain_shifted_letter_is_the_uppercase_character() {
    let event = KeyEvent::new(KeyCode::Char('A'), SHIFT);

    assert_eq!(KeyChord::from_event(event).unwrap(), chord("A"));
    assert_eq!(chord("shift+a"), chord("A"));
    assert_eq!(chord("A").modifiers(), KeyModifiers::NONE);
}

#[test]
fn back_tab_is_shift_tab() {
    let event = KeyEvent::new(KeyCode::BackTab, SHIFT);

    assert_eq!(KeyChord::from_event(event).unwrap(), chord("shift+tab"));
}

#[test]
fn irrelevant_modifiers_are_dropped() {
    let event = KeyEvent::new(KeyCode::Char('s'), CTRL | KeyModifiers::HYPER);

    assert_eq!(KeyChord::from_event(event).unwrap(), chord("ctrl+s"));
}

#[test]
fn key_release_is_not_a_chord() {
    let mut event = KeyEvent::new(KeyCode::Char('s'), CTRL);
    event.kind = KeyEventKind::Release;

    assert_eq!(KeyChord::from_event(event), None);
}

#[test]
fn key_repeat_is_a_chord() {
    let mut event = KeyEvent::new(KeyCode::Char('s'), CTRL);
    event.kind = KeyEventKind::Repeat;

    assert_eq!(KeyChord::from_event(event), Some(chord("ctrl+s")));
}

#[test]
fn named_keys_parse() {
    assert_eq!(chord("space").code(), KeyCode::Char(' '));
    assert_eq!(chord("alt+left").code(), KeyCode::Left);
    assert_eq!(chord("pagedown").code(), KeyCode::PageDown);
    assert_eq!(chord("f12").code(), KeyCode::F(12));
    assert_eq!(chord("ctrl+plus").code(), KeyCode::Char('+'));
}

#[test]
fn errors_name_the_problem() {
    assert_eq!("".parse::<KeyChord>(), Err(KeyParseError::Empty));
    assert_eq!(
        "ctrl+".parse::<KeyChord>(),
        Err(KeyParseError::MissingKey("ctrl+".into()))
    );
    assert_eq!(
        "hyper+a".parse::<KeyChord>(),
        Err(KeyParseError::UnknownModifier("hyper".into()))
    );
    assert_eq!(
        "ctrl+banana".parse::<KeyChord>(),
        Err(KeyParseError::UnknownKey("banana".into()))
    );
    assert_eq!(
        "f25".parse::<KeyChord>(),
        Err(KeyParseError::UnknownKey("f25".into()))
    );
}

#[test]
fn only_unmodified_characters_type_text() {
    assert_eq!(chord("a").typed_char(), Some('a'));
    assert_eq!(chord("A").typed_char(), Some('A'));
    assert_eq!(chord("space").typed_char(), Some(' '));
    assert_eq!(chord("ctrl+a").typed_char(), None);
    assert_eq!(chord("alt+a").typed_char(), None);
    assert_eq!(chord("enter").typed_char(), None);
}
