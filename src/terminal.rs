//! Terminal setup and the capabilities we probe for at startup.

use std::io::stdout;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::event::{
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

/// Enters raw mode and the alternate screen, and enables the kitty keyboard
/// protocol when the terminal supports it.
pub fn init() -> (DefaultTerminal, Capabilities) {
    let terminal = ratatui::init();
    let capabilities = Capabilities {
        keyboard: probe_keyboard(),
    };
    (terminal, capabilities)
}

/// Undoes [`init`]. Safe to call more than once and from a panic hook.
pub fn restore() {
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
