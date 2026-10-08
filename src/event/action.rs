use serde::Deserialize;

use crate::components::editor::Motion;

/// Everything a key, menu or plugin can ask the editor to do. Keymaps bind
/// keys to these, and the command palette lists them.
///
/// The enum is closed except for [`Action::Plugin`], the hole through which
/// plugins register actions of their own.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Action {
    Quit,
    Save,
    Undo,
    Redo,
    InsertText(Box<str>),
    InsertNewline,
    DeleteBackward,
    DeleteForward,
    DeleteWordBackward,
    DeleteWordForward,
    SelectAll,
    Indent,
    Outdent,
    Copy,
    Cut,
    Paste,
    ToggleMouse,
    Move(Motion),
    Select(Motion),
    Plugin(Box<str>),
}
