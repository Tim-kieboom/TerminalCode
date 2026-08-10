use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[cfg(test)]
#[path = "tests/terminal_keys_tests.rs"]
mod tests;

pub fn encode_key(key: &KeyEvent) -> Option<Vec<u8>> {
    if key.modifiers.contains(KeyModifiers::ALT) {
        let KeyCode::Char(ch) = key.code else {
            return None;
        };

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            let code = ctrl_char(ch)?;
            return Some(vec![0x1b, code]);
        }
        return Some(encode_char_with_alt(ch));
    }

    match key.code {
        KeyCode::Char(ch) if key.modifiers.contains(KeyModifiers::CONTROL) => {
            ctrl_char(ch).map(|code| vec![code])
        }
        KeyCode::Char(ch) => Some(ch.to_string().into_bytes()),
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
        KeyCode::F(n) => Some(f_key(n)),
        _ => None,
    }
}

fn encode_char_with_alt(ch: char) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(ch.len_utf8() + 1);
    bytes.push(0x1b);
    bytes.extend(ch.to_string().into_bytes());
    bytes
}

fn ctrl_char(ch: char) -> Option<u8> {
    match ch.to_ascii_lowercase() {
        'a'..='z' => Some(ch.to_ascii_lowercase() as u8 - b'a' + 1),
        '[' => Some(0x1b),
        '\\' => Some(0x1c),
        ']' => Some(0x1d),
        '^' => Some(0x1e),
        '_' => Some(0x1f),
        '@' | '`' => Some(0x00),
        ' ' => Some(0x00),
        _ => None,
    }
}

fn f_key(n: u8) -> Vec<u8> {
    let bytes: &[u8] = match n {
        1 => b"\x1bOP",
        2 => b"\x1bOQ",
        3 => b"\x1bOR",
        4 => b"\x1bOS",
        5 => b"\x1b[15~",
        6 => b"\x1b[17~",
        7 => b"\x1b[18~",
        8 => b"\x1b[19~",
        9 => b"\x1b[20~",
        10 => b"\x1b[21~",
        11 => b"\x1b[23~",
        12 => b"\x1b[24~",
        _ => b"",
    };
    bytes.to_vec()
}
