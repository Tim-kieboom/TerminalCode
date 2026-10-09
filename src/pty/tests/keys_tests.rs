use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::pty::{encode_key, encode_paste};

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

fn bytes(code: KeyCode, modifiers: KeyModifiers) -> Vec<u8> {
    encode_key(key(code, modifiers), false).expect("the key has bytes")
}

const NONE: KeyModifiers = KeyModifiers::NONE;
const CTRL: KeyModifiers = KeyModifiers::CONTROL;
const ALT: KeyModifiers = KeyModifiers::ALT;
const SHIFT: KeyModifiers = KeyModifiers::SHIFT;

#[test]
fn text_is_sent_as_utf8() {
    assert_eq!(bytes(KeyCode::Char('a'), NONE), b"a");
    assert_eq!(bytes(KeyCode::Char('A'), SHIFT), b"A");
    assert_eq!(bytes(KeyCode::Char('é'), NONE), "é".as_bytes());
    assert_eq!(bytes(KeyCode::Char('✓'), NONE), "✓".as_bytes());
}

#[test]
fn control_letters_are_control_characters() {
    assert_eq!(bytes(KeyCode::Char('a'), CTRL), [1]);
    assert_eq!(bytes(KeyCode::Char('c'), CTRL), [3]);
    assert_eq!(bytes(KeyCode::Char('z'), CTRL), [26]);
    assert_eq!(bytes(KeyCode::Char('B'), CTRL | SHIFT), [2]);
}

#[test]
fn control_punctuation_follows_xterm() {
    assert_eq!(bytes(KeyCode::Char(' '), CTRL), [0]);
    assert_eq!(bytes(KeyCode::Char('@'), CTRL), [0]);
    assert_eq!(bytes(KeyCode::Char('['), CTRL), [0x1b]);
    assert_eq!(bytes(KeyCode::Char('\\'), CTRL), [0x1c]);
    assert_eq!(bytes(KeyCode::Char(']'), CTRL), [0x1d]);
    assert_eq!(bytes(KeyCode::Char('_'), CTRL), [0x1f]);
    assert_eq!(bytes(KeyCode::Char('?'), CTRL), [0x7f]);
}

#[test]
fn control_with_a_character_that_has_no_control_form_sends_nothing() {
    assert_eq!(encode_key(key(KeyCode::Char('é'), CTRL), false), None);
    assert_eq!(encode_key(key(KeyCode::Char('!'), CTRL), false), None);
}

#[test]
fn alt_puts_an_escape_in_front() {
    assert_eq!(bytes(KeyCode::Char('f'), ALT), b"\x1bf");
    assert_eq!(bytes(KeyCode::Char('b'), ALT | CTRL), b"\x1b\x02");
    assert_eq!(bytes(KeyCode::Enter, ALT), b"\x1b\r");
    assert_eq!(bytes(KeyCode::Backspace, ALT), b"\x1b\x7f");
}

#[test]
fn the_editing_keys() {
    assert_eq!(bytes(KeyCode::Enter, NONE), b"\r");
    assert_eq!(bytes(KeyCode::Tab, NONE), b"\t");
    assert_eq!(bytes(KeyCode::Tab, SHIFT), b"\x1b[Z");
    assert_eq!(bytes(KeyCode::BackTab, SHIFT), b"\x1b[Z");
    assert_eq!(bytes(KeyCode::Backspace, NONE), [0x7f]);
    assert_eq!(bytes(KeyCode::Backspace, CTRL), [0x08]);
    assert_eq!(bytes(KeyCode::Esc, NONE), [0x1b]);
}

#[test]
fn arrows_home_and_end() {
    assert_eq!(bytes(KeyCode::Up, NONE), b"\x1b[A");
    assert_eq!(bytes(KeyCode::Down, NONE), b"\x1b[B");
    assert_eq!(bytes(KeyCode::Right, NONE), b"\x1b[C");
    assert_eq!(bytes(KeyCode::Left, NONE), b"\x1b[D");
    assert_eq!(bytes(KeyCode::Home, NONE), b"\x1b[H");
    assert_eq!(bytes(KeyCode::End, NONE), b"\x1b[F");
}

#[test]
fn a_program_in_application_cursor_mode_gets_the_other_form() {
    let up = encode_key(key(KeyCode::Up, NONE), true).unwrap();
    let home = encode_key(key(KeyCode::Home, NONE), true).unwrap();

    assert_eq!(up, b"\x1bOA");
    assert_eq!(home, b"\x1bOH");
}

#[test]
fn modifiers_on_cursor_keys_use_xterms_numbers() {
    assert_eq!(bytes(KeyCode::Left, CTRL), b"\x1b[1;5D");
    assert_eq!(bytes(KeyCode::Right, ALT), b"\x1b[1;3C");
    assert_eq!(bytes(KeyCode::Up, SHIFT), b"\x1b[1;2A");
    assert_eq!(bytes(KeyCode::Down, CTRL | SHIFT), b"\x1b[1;6B");
    let in_application_mode = encode_key(key(KeyCode::Left, CTRL), true).unwrap();
    assert_eq!(
        in_application_mode, b"\x1b[1;5D",
        "modifiers win over the mode"
    );
}

#[test]
fn insert_delete_and_page_keys() {
    assert_eq!(bytes(KeyCode::Insert, NONE), b"\x1b[2~");
    assert_eq!(bytes(KeyCode::Delete, NONE), b"\x1b[3~");
    assert_eq!(bytes(KeyCode::PageUp, NONE), b"\x1b[5~");
    assert_eq!(bytes(KeyCode::PageDown, NONE), b"\x1b[6~");
    assert_eq!(bytes(KeyCode::Delete, CTRL), b"\x1b[3;5~");
    assert_eq!(bytes(KeyCode::PageUp, SHIFT), b"\x1b[5;2~");
}

#[test]
fn function_keys() {
    assert_eq!(bytes(KeyCode::F(1), NONE), b"\x1bOP");
    assert_eq!(bytes(KeyCode::F(4), NONE), b"\x1bOS");
    assert_eq!(bytes(KeyCode::F(5), NONE), b"\x1b[15~");
    assert_eq!(bytes(KeyCode::F(10), NONE), b"\x1b[21~");
    assert_eq!(bytes(KeyCode::F(12), NONE), b"\x1b[24~");
    assert_eq!(bytes(KeyCode::F(2), CTRL), b"\x1b[1;5Q");
    assert_eq!(bytes(KeyCode::F(5), SHIFT), b"\x1b[15;2~");
    assert_eq!(encode_key(key(KeyCode::F(13), NONE), false), None);
}

#[test]
fn releases_and_keys_without_bytes_send_nothing() {
    let mut release = key(KeyCode::Char('a'), NONE);
    release.kind = KeyEventKind::Release;

    assert_eq!(encode_key(release, false), None);
    assert_eq!(encode_key(key(KeyCode::CapsLock, NONE), false), None);
    assert_eq!(encode_key(key(KeyCode::Null, NONE), false), None);
}

#[test]
fn a_repeated_key_counts_as_a_press() {
    let mut repeat = key(KeyCode::Char('a'), NONE);
    repeat.kind = KeyEventKind::Repeat;

    assert_eq!(encode_key(repeat, false), Some(b"a".to_vec()));
}

#[test]
fn a_plain_paste_turns_line_breaks_into_carriage_returns() {
    assert_eq!(encode_paste("one\ntwo\r\nthree", false), b"one\rtwo\rthree");
}

#[test]
fn a_bracketed_paste_is_wrapped_and_left_as_it_is() {
    assert_eq!(
        encode_paste("one\ntwo", true),
        b"\x1b[200~one\ntwo\x1b[201~"
    );
}

#[test]
fn pasted_text_cannot_end_its_own_bracketed_paste() {
    let bytes = encode_paste("safe\x1b[201~rm -rf /", true);

    assert_eq!(bytes, b"\x1b[200~saferm -rf /\x1b[201~");
}
