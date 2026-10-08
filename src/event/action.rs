use serde::Deserialize;

use crate::components::editor::Motion;
use crate::components::workspace::FocusDirection;

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
    NewFile,
    CloseTab,
    NextTab,
    PreviousTab,
    SplitRight,
    SplitDown,
    FocusNextPane,
    FocusPane(FocusDirection),
    Move(Motion),
    Select(Motion),
    Plugin(Box<str>),
}

impl Action {
    /// Whether the action works on an open file, and so has nothing to do
    /// when every tab is closed.
    pub(crate) fn needs_editor(&self) -> bool {
        !matches!(
            self,
            Self::Quit
                | Self::ToggleMouse
                | Self::NewFile
                | Self::CloseTab
                | Self::NextTab
                | Self::PreviousTab
                | Self::SplitRight
                | Self::SplitDown
                | Self::FocusNextPane
                | Self::FocusPane(_)
                | Self::Plugin(_)
        )
    }
}
