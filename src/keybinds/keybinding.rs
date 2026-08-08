use std::write;

use anyhow::bail;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[cfg(test)]
#[path = "tests/keybinding_tests.rs"]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBinding {
    pub modifiers: KeyModifiers,
    pub keycode: KeyCode,
}

impl std::fmt::Display for KeyBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts: Vec<&str> = Vec::new();
        if self.modifiers.contains(KeyModifiers::CONTROL) {
            parts.push("Ctrl");
        }
        if self.modifiers.contains(KeyModifiers::ALT) {
            parts.push("Alt");
        }
        if self.modifiers.contains(KeyModifiers::SHIFT) {
            parts.push("Shift");
        }

        let buf;
        match self.keycode {
            KeyCode::Char(' ') => parts.push("Space"),
            KeyCode::Char(c) => {
                buf = c.to_uppercase().to_string();
                parts.push(&buf);
            }
            KeyCode::Tab => parts.push("Tab"),
            KeyCode::Enter => parts.push("Enter"),
            KeyCode::Esc => parts.push("Esc"),
            KeyCode::F(n) => {
                buf = format!("F{n}");
                parts.push(&buf);
            }
            KeyCode::Up => parts.push("Up"),
            KeyCode::Down => parts.push("Down"),
            KeyCode::Left => parts.push("Left"),
            KeyCode::Right => parts.push("Right"),
            KeyCode::Backspace => parts.push("Backspace"),
            KeyCode::Delete => parts.push("Delete"),
            KeyCode::Home => parts.push("Home"),
            KeyCode::End => parts.push("End"),
            KeyCode::PageUp => parts.push("PageUp"),
            KeyCode::PageDown => parts.push("PageDown"),
            _ => parts.push("?"),
        }
        write!(f, "{}", parts.join("+"))
    }
}

impl KeyBinding {
    pub fn parse(string: &str) -> anyhow::Result<Self> {
        let mut modifiers = KeyModifiers::NONE;
        let mut keycode = None;

        for part in string.split('+') {
            match part.to_lowercase().as_str() {
                "ctrl" => modifiers |= KeyModifiers::CONTROL,
                "alt" => modifiers |= KeyModifiers::ALT,
                "shift" => modifiers |= KeyModifiers::SHIFT,
                "tab" => keycode = Some(KeyCode::Tab),
                "enter" => keycode = Some(KeyCode::Enter),
                "esc" => keycode = Some(KeyCode::Esc),
                "up" => keycode = Some(KeyCode::Up),
                "down" => keycode = Some(KeyCode::Down),
                "left" => keycode = Some(KeyCode::Left),
                "right" => keycode = Some(KeyCode::Right),
                "backspace" => keycode = Some(KeyCode::Backspace),
                "delete" => keycode = Some(KeyCode::Delete),
                "home" => keycode = Some(KeyCode::Home),
                "end" => keycode = Some(KeyCode::End),
                "pageup" => keycode = Some(KeyCode::PageUp),
                "pagedown" => keycode = Some(KeyCode::PageDown),
                "f1" => keycode = Some(KeyCode::F(1)),
                "f2" => keycode = Some(KeyCode::F(2)),
                "f3" => keycode = Some(KeyCode::F(3)),
                "f4" => keycode = Some(KeyCode::F(4)),
                "f5" => keycode = Some(KeyCode::F(5)),
                "f6" => keycode = Some(KeyCode::F(6)),
                "f7" => keycode = Some(KeyCode::F(7)),
                "f8" => keycode = Some(KeyCode::F(8)),
                "f9" => keycode = Some(KeyCode::F(9)),
                "f10" => keycode = Some(KeyCode::F(10)),
                "f11" => keycode = Some(KeyCode::F(11)),
                "f12" => keycode = Some(KeyCode::F(12)),
                "space" => keycode = Some(KeyCode::Char(' ')),
                s if s.len() == 1 => {
                    keycode = Some(KeyCode::Char(s.chars().next().unwrap()));
                }
                _ => bail!("{part:?} is invalid value for keybind"),
            }
        }

        keycode
            .map(|k| KeyBinding {
                modifiers,
                keycode: k,
            })
            .ok_or(anyhow::anyhow!("keybind does not have a keycode"))
    }

    pub fn matches(&self, key: &KeyEvent) -> bool {
        if key.modifiers != self.modifiers {
            return false;
        }
        match (self.keycode, key.code) {
            (KeyCode::Char(a), KeyCode::Char(b)) => a.eq_ignore_ascii_case(&b),
            _ => self.keycode == key.code,
        }
    }
}
impl From<KeyEvent> for KeyBinding {
    fn from(value: KeyEvent) -> Self {
        KeyBinding {
            modifiers: value.modifiers,
            keycode: value.code,
        }
    }
}
