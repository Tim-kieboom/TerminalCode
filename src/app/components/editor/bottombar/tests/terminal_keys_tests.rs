use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::encode_key;

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

#[test]
fn plain_chars_encode_as_utf8() {
    assert_eq!(
        encode_key(&key(KeyCode::Char('a'), KeyModifiers::NONE)),
        Some(b"a".to_vec())
    );
    assert_eq!(
        encode_key(&key(KeyCode::Char('A'), KeyModifiers::SHIFT)),
        Some(b"A".to_vec())
    );
    assert_eq!(
        encode_key(&key(KeyCode::Char(' '), KeyModifiers::NONE)),
        Some(b" ".to_vec())
    );
}

#[test]
fn ctrl_letters_encode_as_control_bytes() {
    assert_eq!(
        encode_key(&key(KeyCode::Char('a'), KeyModifiers::CONTROL)),
        Some(vec![0x01])
    );
    assert_eq!(
        encode_key(&key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        Some(vec![0x03])
    );
    assert_eq!(
        encode_key(&key(KeyCode::Char('z'), KeyModifiers::CONTROL)),
        Some(vec![0x1a])
    );
}

#[test]
fn ctrl_shift_letters_encode_as_control_bytes() {
    let modifiers = KeyModifiers::CONTROL | KeyModifiers::SHIFT;
    assert_eq!(
        encode_key(&key(KeyCode::Char('A'), modifiers)),
        Some(vec![0x01])
    );
}

#[test]
fn ctrl_punctuation_encodes_common_codes() {
    assert_eq!(
        encode_key(&key(KeyCode::Char('['), KeyModifiers::CONTROL)),
        Some(vec![0x1b])
    );
    assert_eq!(
        encode_key(&key(KeyCode::Char('\\'), KeyModifiers::CONTROL)),
        Some(vec![0x1c])
    );
    assert_eq!(
        encode_key(&key(KeyCode::Char(']'), KeyModifiers::CONTROL)),
        Some(vec![0x1d])
    );
    assert_eq!(
        encode_key(&key(KeyCode::Char('_'), KeyModifiers::CONTROL)),
        Some(vec![0x1f])
    );
}

#[test]
fn alt_chars_prefix_with_escape() {
    assert_eq!(
        encode_key(&key(KeyCode::Char('x'), KeyModifiers::ALT)),
        Some(vec![0x1b, b'x'])
    );
}

#[test]
fn special_keys_encode_as_expected() {
    assert_eq!(
        encode_key(&key(KeyCode::Enter, KeyModifiers::NONE)),
        Some(vec![b'\r'])
    );
    assert_eq!(
        encode_key(&key(KeyCode::Tab, KeyModifiers::NONE)),
        Some(vec![b'\t'])
    );
    assert_eq!(
        encode_key(&key(KeyCode::Backspace, KeyModifiers::NONE)),
        Some(vec![0x7f])
    );
    assert_eq!(
        encode_key(&key(KeyCode::Esc, KeyModifiers::NONE)),
        Some(vec![0x1b])
    );
}

#[test]
fn navigation_keys_encode_as_escape_sequences() {
    assert_eq!(
        encode_key(&key(KeyCode::Left, KeyModifiers::NONE)),
        Some(b"\x1b[D".to_vec())
    );
    assert_eq!(
        encode_key(&key(KeyCode::Right, KeyModifiers::NONE)),
        Some(b"\x1b[C".to_vec())
    );
    assert_eq!(
        encode_key(&key(KeyCode::Up, KeyModifiers::NONE)),
        Some(b"\x1b[A".to_vec())
    );
    assert_eq!(
        encode_key(&key(KeyCode::Down, KeyModifiers::NONE)),
        Some(b"\x1b[B".to_vec())
    );
    assert_eq!(
        encode_key(&key(KeyCode::Home, KeyModifiers::NONE)),
        Some(b"\x1b[H".to_vec())
    );
    assert_eq!(
        encode_key(&key(KeyCode::End, KeyModifiers::NONE)),
        Some(b"\x1b[F".to_vec())
    );
    assert_eq!(
        encode_key(&key(KeyCode::PageUp, KeyModifiers::NONE)),
        Some(b"\x1b[5~".to_vec())
    );
    assert_eq!(
        encode_key(&key(KeyCode::PageDown, KeyModifiers::NONE)),
        Some(b"\x1b[6~".to_vec())
    );
    assert_eq!(
        encode_key(&key(KeyCode::Delete, KeyModifiers::NONE)),
        Some(b"\x1b[3~".to_vec())
    );
    assert_eq!(
        encode_key(&key(KeyCode::Insert, KeyModifiers::NONE)),
        Some(b"\x1b[2~".to_vec())
    );
}

#[test]
fn function_keys_encode_appropriately() {
    assert_eq!(
        encode_key(&key(KeyCode::F(1), KeyModifiers::NONE)),
        Some(b"\x1bOP".to_vec())
    );
    assert_eq!(
        encode_key(&key(KeyCode::F(4), KeyModifiers::NONE)),
        Some(b"\x1bOS".to_vec())
    );
    assert_eq!(
        encode_key(&key(KeyCode::F(5), KeyModifiers::NONE)),
        Some(b"\x1b[15~".to_vec())
    );
    assert_eq!(
        encode_key(&key(KeyCode::F(12), KeyModifiers::NONE)),
        Some(b"\x1b[24~".to_vec())
    );
}

#[test]
fn unknown_keys_are_not_encoded() {
    assert_eq!(encode_key(&key(KeyCode::Null, KeyModifiers::NONE)), None);
    assert_eq!(encode_key(&key(KeyCode::CapsLock, KeyModifiers::ALT)), None);
}
