use super::KeyBinding;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

#[test]
fn parse_plain_char() {
    let binding = KeyBinding::parse("a").unwrap();
    assert_eq!(binding.keycode, KeyCode::Char('a'));
    assert_eq!(binding.modifiers, KeyModifiers::NONE);
}

#[test]
fn parse_is_case_insensitive() {
    let binding = KeyBinding::parse("CTRL+SHIFT+A").unwrap();
    assert!(binding.modifiers.contains(KeyModifiers::CONTROL));
    assert!(binding.modifiers.contains(KeyModifiers::SHIFT));
    assert_eq!(binding.keycode, KeyCode::Char('a'));
}

#[test]
fn parse_missing_keycode_is_err() {
    assert!(KeyBinding::parse("Ctrl+").is_err());
}

#[test]
fn parse_unknown_part_is_err() {
    assert!(KeyBinding::parse("nonsense").is_err());
    assert!(KeyBinding::parse("Ctrl+bogus").is_err());
}

#[test]
fn display_round_trips_special_keys() {
    for key in [
        "Tab",
        "Enter",
        "Esc",
        "Up",
        "Down",
        "Left",
        "Right",
        "Backspace",
        "Delete",
        "Home",
        "End",
        "PageUp",
        "PageDown",
        "Space",
    ] {
        let binding = KeyBinding::parse(key).unwrap();
        assert_eq!(binding.to_string(), key);
    }
}

#[test]
fn display_round_trips_function_keys() {
    for n in 1..=12 {
        let key = format!("F{n}");
        let binding = KeyBinding::parse(&key).unwrap();
        assert_eq!(binding.to_string(), key);
    }
}

#[test]
fn display_orders_modifiers() {
    let binding = KeyBinding::parse("Alt+Shift+Ctrl+D").unwrap();
    assert_eq!(binding.to_string(), "Ctrl+Alt+Shift+D");
}

#[test]
fn display_uppercases_char_keys() {
    let binding = KeyBinding::parse("q").unwrap();
    assert_eq!(binding.to_string(), "Q");
}

#[test]
fn matches_char_is_case_insensitive() {
    let binding = KeyBinding::parse("q").unwrap();
    assert!(binding.matches(&key(KeyCode::Char('Q'), KeyModifiers::NONE)));
    assert!(binding.matches(&key(KeyCode::Char('q'), KeyModifiers::NONE)));
}

#[test]
fn matches_requires_equal_modifiers() {
    let binding = KeyBinding::parse("Ctrl+Q").unwrap();
    assert!(binding.matches(&key(KeyCode::Char('Q'), KeyModifiers::CONTROL)));
    assert!(!binding.matches(&key(KeyCode::Char('Q'), KeyModifiers::NONE)));
    assert!(!binding.matches(&key(KeyCode::Char('Q'), KeyModifiers::SHIFT)));
}

#[test]
fn matches_non_char_keys_exactly() {
    let binding = KeyBinding::parse("Enter").unwrap();
    assert!(binding.matches(&key(KeyCode::Enter, KeyModifiers::NONE)));
    assert!(!binding.matches(&key(KeyCode::Esc, KeyModifiers::NONE)));
}
