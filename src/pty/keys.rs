//! What a terminal program expects to receive for a key: the bytes an xterm
//! would send.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

const ESC: u8 = 0x1b;

/// The bytes for a key press, or `None` for a key a terminal has no bytes for
/// (and for releases). `application_cursor` is the program's choice of what
/// the arrow keys, Home and End send (`ESC O A` instead of `ESC [ A`).
pub(crate) fn encode_key(key: KeyEvent, application_cursor: bool) -> Option<Vec<u8>> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    // xterm's modifier number: 1 plus shift 1, alt 2, control 4.
    let modifier = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(ctrl);

    let bytes = match key.code {
        KeyCode::Char(c) => return char_bytes(c, ctrl, alt),
        KeyCode::Enter => with_alt(alt, vec![b'\r']),
        KeyCode::Tab if shift => vec![ESC, b'[', b'Z'],
        KeyCode::Tab => with_alt(alt, vec![b'\t']),
        KeyCode::BackTab => vec![ESC, b'[', b'Z'],
        KeyCode::Backspace if ctrl => vec![0x08],
        KeyCode::Backspace => with_alt(alt, vec![0x7f]),
        KeyCode::Esc => with_alt(alt, vec![ESC]),
        KeyCode::Up => cursor_key(b'A', modifier, application_cursor),
        KeyCode::Down => cursor_key(b'B', modifier, application_cursor),
        KeyCode::Right => cursor_key(b'C', modifier, application_cursor),
        KeyCode::Left => cursor_key(b'D', modifier, application_cursor),
        KeyCode::Home => cursor_key(b'H', modifier, application_cursor),
        KeyCode::End => cursor_key(b'F', modifier, application_cursor),
        KeyCode::Insert => tilde_key(2, modifier),
        KeyCode::Delete => tilde_key(3, modifier),
        KeyCode::PageUp => tilde_key(5, modifier),
        KeyCode::PageDown => tilde_key(6, modifier),
        KeyCode::F(number) => function_key(number, modifier)?,
        _ => return None,
    };
    Some(bytes)
}

/// The bytes for pasted text. A program that asked for bracketed paste gets the
/// text between markers, and with the end marker taken out of it so the text
/// cannot end its own paste early; one that did not gets the text with line
/// breaks as carriage returns, which is what Enter sends.
pub(crate) fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
    if bracketed {
        let safe = text.replace("\x1b[201~", "");
        let mut bytes = b"\x1b[200~".to_vec();
        bytes.extend_from_slice(safe.as_bytes());
        bytes.extend_from_slice(b"\x1b[201~");
        return bytes;
    }
    text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
}

fn char_bytes(c: char, ctrl: bool, alt: bool) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    if alt {
        bytes.push(ESC);
    }
    if ctrl {
        bytes.push(control_byte(c)?);
    } else {
        let mut buffer = [0u8; 4];
        bytes.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
    }
    Some(bytes)
}

/// The control character for `c` held with Ctrl: `ctrl+a` is 1 and so on.
fn control_byte(c: char) -> Option<u8> {
    Some(match c {
        'a'..='z' => c as u8 - b'a' + 1,
        'A'..='Z' => c as u8 - b'A' + 1,
        '@' | ' ' | '2' => 0,
        '[' | '3' => 0x1b,
        '\\' | '4' => 0x1c,
        ']' | '5' => 0x1d,
        '^' | '6' => 0x1e,
        '_' | '-' | '7' => 0x1f,
        '?' | '8' => 0x7f,
        _ => return None,
    })
}

fn with_alt(alt: bool, mut bytes: Vec<u8>) -> Vec<u8> {
    if alt {
        bytes.insert(0, ESC);
    }
    bytes
}

/// Arrow keys, Home and End: `ESC [ A`, `ESC O A` in application mode, and
/// `ESC [ 1 ; 5 A` with modifiers.
fn cursor_key(letter: u8, modifier: u8, application: bool) -> Vec<u8> {
    if modifier > 1 {
        return format!("\x1b[1;{modifier}{}", letter as char).into_bytes();
    }
    let introducer = if application { b'O' } else { b'[' };
    vec![ESC, introducer, letter]
}

/// Insert, Delete, Page Up and Page Down: `ESC [ 3 ~`, `ESC [ 3 ; 5 ~`.
fn tilde_key(number: u8, modifier: u8) -> Vec<u8> {
    if modifier > 1 {
        return format!("\x1b[{number};{modifier}~").into_bytes();
    }
    format!("\x1b[{number}~").into_bytes()
}

fn function_key(number: u8, modifier: u8) -> Option<Vec<u8>> {
    let letter = match number {
        1 => b'P',
        2 => b'Q',
        3 => b'R',
        4 => b'S',
        _ => {
            let code = match number {
                5 => 15,
                6 => 17,
                7 => 18,
                8 => 19,
                9 => 20,
                10 => 21,
                11 => 23,
                12 => 24,
                _ => return None,
            };
            return Some(tilde_key(code, modifier));
        }
    };
    if modifier > 1 {
        return Some(format!("\x1b[1;{modifier}{}", letter as char).into_bytes());
    }
    Some(vec![ESC, b'O', letter])
}
