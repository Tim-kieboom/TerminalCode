use std::fmt;
use std::str::FromStr;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use thiserror::Error;

/// Modifiers a binding can use. Everything else crossterm reports (caps lock,
/// keypad, ...) is dropped.
const BINDABLE: KeyModifiers = KeyModifiers::CONTROL
    .union(KeyModifiers::ALT)
    .union(KeyModifiers::SHIFT)
    .union(KeyModifiers::SUPER);

/// A key plus modifiers in canonical form, so that what a terminal reports and
/// what a keymap file says compare equal:
///
/// - With Ctrl, Alt or Super held, letter case is carried by Shift: the chord
///   is `Char('p')` with `SHIFT`, never `Char('P')`.
/// - Otherwise Shift is part of the character: `Shift+a` is `Char('A')`.
/// - Back-tab is `Tab` with `SHIFT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct KeyChord {
    code: KeyCode,
    modifiers: KeyModifiers,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum KeyParseError {
    #[error("empty key")]
    Empty,
    #[error("`{0}` has no key after the modifiers")]
    MissingKey(Box<str>),
    #[error("unknown modifier `{0}`")]
    UnknownModifier(Box<str>),
    #[error("unknown key `{0}`")]
    UnknownKey(Box<str>),
}

impl KeyChord {
    pub(crate) fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        let modifiers = modifiers & BINDABLE;
        let (code, modifiers) = match code {
            KeyCode::BackTab => (KeyCode::Tab, modifiers | KeyModifiers::SHIFT),
            KeyCode::Char(c) => normalize_char(c, modifiers),
            other => (other, modifiers),
        };
        Self { code, modifiers }
    }

    /// The chord a terminal key event stands for. Releases are not chords.
    pub(crate) fn from_event(event: KeyEvent) -> Option<Self> {
        if event.kind == KeyEventKind::Release {
            return None;
        }
        Some(Self::new(event.code, event.modifiers))
    }

    /// The character this chord types into text, if it types one.
    pub(crate) fn typed_char(&self) -> Option<char> {
        match self.code {
            KeyCode::Char(c) if self.modifiers.is_empty() => Some(c),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn code(&self) -> KeyCode {
        self.code
    }

    #[cfg(test)]
    pub(crate) fn modifiers(&self) -> KeyModifiers {
        self.modifiers
    }
}

fn normalize_char(c: char, modifiers: KeyModifiers) -> (KeyCode, KeyModifiers) {
    let shortcut =
        modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER);
    if shortcut {
        return match single_case_mapping(c.to_lowercase()) {
            Some(lower) if lower != c => (KeyCode::Char(lower), modifiers | KeyModifiers::SHIFT),
            _ => (KeyCode::Char(c), modifiers),
        };
    }

    if !modifiers.contains(KeyModifiers::SHIFT) {
        return (KeyCode::Char(c), modifiers);
    }
    let shifted = single_case_mapping(c.to_uppercase()).unwrap_or(c);
    (KeyCode::Char(shifted), modifiers - KeyModifiers::SHIFT)
}

/// The mapped char when the case mapping is a single char (`ß` -> `SS` is not).
fn single_case_mapping(mut mapping: impl Iterator<Item = char>) -> Option<char> {
    let first = mapping.next()?;
    mapping.next().is_none().then_some(first)
}

impl fmt::Display for KeyChord {
    /// The text form [`FromStr`] reads back: `ctrl+shift+p`, `alt+left`, `f5`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (flag, name) in [
            (KeyModifiers::CONTROL, "ctrl"),
            (KeyModifiers::ALT, "alt"),
            (KeyModifiers::SHIFT, "shift"),
            (KeyModifiers::SUPER, "super"),
        ] {
            if self.modifiers.contains(flag) {
                write!(f, "{name}+")?;
            }
        }
        match self.code {
            KeyCode::Char(' ') => f.write_str("space"),
            KeyCode::Char('+') => f.write_str("plus"),
            KeyCode::Char(c) => write!(f, "{c}"),
            KeyCode::F(n) => write!(f, "f{n}"),
            KeyCode::Enter => f.write_str("enter"),
            KeyCode::Esc => f.write_str("esc"),
            KeyCode::Tab => f.write_str("tab"),
            KeyCode::Backspace => f.write_str("backspace"),
            KeyCode::Delete => f.write_str("delete"),
            KeyCode::Insert => f.write_str("insert"),
            KeyCode::Home => f.write_str("home"),
            KeyCode::End => f.write_str("end"),
            KeyCode::PageUp => f.write_str("pageup"),
            KeyCode::PageDown => f.write_str("pagedown"),
            KeyCode::Up => f.write_str("up"),
            KeyCode::Down => f.write_str("down"),
            KeyCode::Left => f.write_str("left"),
            KeyCode::Right => f.write_str("right"),
            other => write!(f, "{other:?}"),
        }
    }
}

impl FromStr for KeyChord {
    type Err = KeyParseError;

    /// Parses `ctrl+shift+p`, `alt+left`, `f5`, `space`, ... Modifier and key
    /// names are case-insensitive; single characters keep their case.
    fn from_str(source: &str) -> Result<Self, Self::Err> {
        if source.is_empty() {
            return Err(KeyParseError::Empty);
        }
        let mut tokens = source.split('+').collect::<Vec<_>>();
        let key = tokens.pop().filter(|key| !key.is_empty());
        let Some(key) = key else {
            return Err(KeyParseError::MissingKey(source.into()));
        };

        let mut modifiers = KeyModifiers::NONE;
        for token in tokens {
            modifiers |= parse_modifier(token)?;
        }
        Ok(Self::new(parse_key(key)?, modifiers))
    }
}

fn parse_modifier(token: &str) -> Result<KeyModifiers, KeyParseError> {
    match token.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Ok(KeyModifiers::CONTROL),
        "alt" | "option" => Ok(KeyModifiers::ALT),
        "shift" => Ok(KeyModifiers::SHIFT),
        "super" | "cmd" | "win" | "meta" => Ok(KeyModifiers::SUPER),
        _ => Err(KeyParseError::UnknownModifier(token.into())),
    }
}

fn parse_key(token: &str) -> Result<KeyCode, KeyParseError> {
    let mut chars = token.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Ok(KeyCode::Char(c));
    }

    let name = token.to_ascii_lowercase();
    if let Some(number) = name.strip_prefix('f') {
        return match number.parse::<u8>() {
            Ok(n @ 1..=24) => Ok(KeyCode::F(n)),
            _ => Err(KeyParseError::UnknownKey(token.into())),
        };
    }

    match name.as_str() {
        "enter" | "return" => Ok(KeyCode::Enter),
        "esc" | "escape" => Ok(KeyCode::Esc),
        "tab" => Ok(KeyCode::Tab),
        "backtab" => Ok(KeyCode::BackTab),
        "backspace" => Ok(KeyCode::Backspace),
        "delete" | "del" => Ok(KeyCode::Delete),
        "insert" | "ins" => Ok(KeyCode::Insert),
        "home" => Ok(KeyCode::Home),
        "end" => Ok(KeyCode::End),
        "pageup" => Ok(KeyCode::PageUp),
        "pagedown" => Ok(KeyCode::PageDown),
        "up" => Ok(KeyCode::Up),
        "down" => Ok(KeyCode::Down),
        "left" => Ok(KeyCode::Left),
        "right" => Ok(KeyCode::Right),
        "space" => Ok(KeyCode::Char(' ')),
        "plus" => Ok(KeyCode::Char('+')),
        _ => Err(KeyParseError::UnknownKey(token.into())),
    }
}
