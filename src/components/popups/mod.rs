//! The overlays that take the keyboard: prompts, menus, the palette and the finders.
//! What each does with keys and mouse is in `app/popups/`.

pub mod action_list;
pub mod confirm;
pub mod definition_list;
pub mod diagnostic_popup;
pub mod find;
pub mod finder;
pub mod key_capture;
pub mod menu;
pub mod name_prompt;
pub mod palette;
pub(crate) mod pick_list;
pub mod plugin_list;
pub mod plugin_readme;
pub mod quit_prompt;
pub mod root_prompt;
pub mod search;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// The key of a press that is plain: not a release, and held with neither
/// ctrl nor alt. Lists that only take arrows, Enter, Esc and characters start
/// their `from_key` with it.
pub(crate) fn plain_key(key: KeyEvent) -> Option<KeyCode> {
    let held = KeyModifiers::CONTROL | KeyModifiers::ALT;
    match key.kind == KeyEventKind::Release || key.modifiers.intersects(held) {
        true => None,
        false => Some(key.code),
    }
}

/// The key of a press that is not a release or held with alt, and whether ctrl
/// is held: for prompts where ctrl changes what a key does.
pub(crate) fn key_with_ctrl(key: KeyEvent) -> Option<(KeyCode, bool)> {
    if key.kind == KeyEventKind::Release || key.modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    Some((key.code, key.modifiers.contains(KeyModifiers::CONTROL)))
}
