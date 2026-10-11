//! What a terminal program that asked for the mouse receives: xterm's mouse
//! reports.

use crossterm::event::{KeyModifiers, MouseButton, MouseEventKind};
use vt100::{MouseProtocolEncoding, MouseProtocolMode};

/// The report for a mouse event at `column` and `row` (counted from 0, inside
/// the program's screen), or `None` when the program did not ask for that kind
/// of event or the position does not fit the encoding it chose.
pub(crate) fn encode_mouse(
    kind: MouseEventKind,
    modifiers: KeyModifiers,
    column: u16,
    row: u16,
    mode: MouseProtocolMode,
    encoding: MouseProtocolEncoding,
) -> Option<Vec<u8>> {
    let (button, release) = match kind {
        MouseEventKind::Down(button) => (button_number(button), false),
        MouseEventKind::Up(button) => (button_number(button), true),
        MouseEventKind::Drag(button) => (button_number(button) + MOTION, false),
        MouseEventKind::Moved => (3 + MOTION, false),
        MouseEventKind::ScrollUp => (WHEEL, false),
        MouseEventKind::ScrollDown => (WHEEL + 1, false),
        MouseEventKind::ScrollLeft => (WHEEL + 2, false),
        MouseEventKind::ScrollRight => (WHEEL + 3, false),
    };
    let is_motion = button & MOTION != 0;
    let wanted = match mode {
        MouseProtocolMode::None => false,
        // X10: presses only.
        MouseProtocolMode::Press => !release && !is_motion,
        MouseProtocolMode::PressRelease => !is_motion,
        MouseProtocolMode::ButtonMotion => !is_motion || button & 3 != 3,
        MouseProtocolMode::AnyMotion => true,
    };
    if !wanted {
        return None;
    }
    // X10 mode knows no modifiers.
    let modifier_bits = if mode == MouseProtocolMode::Press {
        0
    } else {
        4 * u8::from(modifiers.contains(KeyModifiers::SHIFT))
            + 8 * u8::from(modifiers.contains(KeyModifiers::ALT))
            + 16 * u8::from(modifiers.contains(KeyModifiers::CONTROL))
    };
    let (x, y) = (u32::from(column) + 1, u32::from(row) + 1);

    match encoding {
        MouseProtocolEncoding::Sgr => {
            let end = if release { 'm' } else { 'M' };
            Some(format!("\x1b[<{};{x};{y}{end}", button + modifier_bits).into_bytes())
        }
        MouseProtocolEncoding::Default | MouseProtocolEncoding::Utf8 => {
            // Without SGR a release has no button: it is button 3.
            let code = if release { 3 } else { button } + modifier_bits;
            let mut bytes = b"\x1b[M".to_vec();
            for value in [u32::from(code), x, y] {
                let value = value + 32;
                if encoding == MouseProtocolEncoding::Utf8 {
                    bytes.extend_from_slice(char::from_u32(value)?.to_string().as_bytes());
                } else {
                    bytes.push(u8::try_from(value).ok()?);
                }
            }
            Some(bytes)
        }
    }
}

/// Added to the button for a move with a button held (or 3 for none).
const MOTION: u8 = 32;
/// The code of a wheel notch up; down, left and right follow.
const WHEEL: u8 = 64;

fn button_number(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    }
}
