//! Terminal setup and the capabilities we probe for at startup.

use std::io::stdout;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::supports_keyboard_enhancement;
use ratatui::DefaultTerminal;

/// Whether the terminal reports modifier keys unambiguously.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum KeyboardSupport {
    /// The kitty keyboard protocol is available: `ctrl+shift+p`, `ctrl+enter`
    /// and friends arrive as they were pressed.
    Enhanced,
    /// Plain xterm-style input: several modifier combinations are lost or
    /// reported as other keys, so the keymap adds fallbacks.
    #[default]
    Legacy,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub keyboard: KeyboardSupport,
}

/// Set while the kitty keyboard flags are pushed, so [`restore`] (also called
/// from the panic hook) knows whether to pop them.
static KEYBOARD_ENHANCED: AtomicBool = AtomicBool::new(false);
static BRACKETED_PASTE: AtomicBool = AtomicBool::new(false);
static MOUSE_CAPTURED: AtomicBool = AtomicBool::new(false);

/// Enters raw mode and the alternate screen, and enables the kitty keyboard
/// protocol when the terminal supports it.
pub fn init() -> (DefaultTerminal, Capabilities) {
    let terminal = ratatui::init();
    if execute!(stdout(), EnableBracketedPaste).is_ok() {
        BRACKETED_PASTE.store(true, Ordering::SeqCst);
    }
    set_mouse_capture(true);
    let capabilities = Capabilities {
        keyboard: probe_keyboard(),
    };
    (terminal, capabilities)
}

/// Undoes [`init`]. Safe to call more than once and from a panic hook.
pub fn restore() {
    if MOUSE_CAPTURED.swap(false, Ordering::SeqCst) {
        let _ = execute!(stdout(), DisableMouseCapture);
    }
    if BRACKETED_PASTE.swap(false, Ordering::SeqCst) {
        let _ = execute!(stdout(), DisableBracketedPaste);
    }
    if KEYBOARD_ENHANCED.swap(false, Ordering::SeqCst) {
        // Nothing useful to do if the terminal is already gone.
        let _ = execute!(stdout(), PopKeyboardEnhancementFlags);
    }
    ratatui::restore();
}

fn probe_keyboard() -> KeyboardSupport {
    if !supports_keyboard_enhancement().unwrap_or(false) {
        return KeyboardSupport::Legacy;
    }
    let flags = KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES;
    if execute!(stdout(), PushKeyboardEnhancementFlags(flags)).is_err() {
        return KeyboardSupport::Legacy;
    }
    KEYBOARD_ENHANCED.store(true, Ordering::SeqCst);
    KeyboardSupport::Enhanced
}

/// Turns mouse reporting on or off. With it on, clicks, drags and the wheel
/// reach the editor; the terminal's own text selection then needs a modifier
/// (usually Shift).
pub fn set_mouse_capture(enabled: bool) {
    let result = match enabled {
        true => execute!(stdout(), EnableMouseCapture),
        false => execute!(stdout(), DisableMouseCapture),
    };
    MOUSE_CAPTURED.store(enabled && result.is_ok(), Ordering::SeqCst);
}

/// How long to wait for the terminal to answer a color query. Terminals that
/// cannot be asked usually say so well before this; the wait is for slow
/// links such as SSH.
const COLOR_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);

/// The terminal's own background color, asked with an OSC 11 query. Call it
/// after [`init`] and before input is being read. `None` if the terminal does
/// not answer.
pub(crate) fn query_background() -> Option<crate::ui::theme::Rgb> {
    let mut options = terminal_colorsaurus::QueryOptions::default();
    options.timeout = COLOR_QUERY_TIMEOUT;
    let color = terminal_colorsaurus::background_color(options).ok()?;
    let (r, g, b) = color.scale_to_8bit();
    Some(crate::ui::theme::Rgb { r, g, b })
}
