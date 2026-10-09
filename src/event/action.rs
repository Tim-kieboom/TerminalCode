use serde::Deserialize;

use crate::components::editor::Motion;
use crate::components::explorer::ExplorerCommand;
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
    CommandPalette,
    /// Moves the keyboard to the explorer, or back to the editor if it is there.
    FocusExplorer,
    /// Shows the explorer if it is hidden, hides it if it is shown.
    ToggleExplorer,
    ToggleStatusBar,
    /// Shows or hides the view of the plugin with this id.
    TogglePluginView(Box<str>),
    /// Opens the file finder.
    FindFile,
    /// Opens the project search.
    FindInProject,
    /// Opens the find bar for the open file.
    FindInFile,
    /// Deletes the explorer's selected file or folder, to the trash.
    DeleteSelected,
    Explorer(ExplorerCommand),
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
    /// Whether running the action puts the keyboard back in the editor: the
    /// ones that open, close or switch tabs and panes.
    pub(crate) fn focuses_editor(&self) -> bool {
        matches!(
            self,
            Self::NewFile
                | Self::CloseTab
                | Self::NextTab
                | Self::PreviousTab
                | Self::SplitRight
                | Self::SplitDown
                | Self::FocusNextPane
                | Self::FocusPane(_)
        )
    }

    /// Whether the action works on an open file, and so has nothing to do
    /// when every tab is closed.
    pub(crate) fn needs_editor(&self) -> bool {
        !matches!(
            self,
            Self::Quit
                | Self::CommandPalette
                | Self::FocusExplorer
                | Self::ToggleExplorer
                | Self::ToggleStatusBar
                | Self::TogglePluginView(_)
                | Self::FindFile
                | Self::FindInProject
                | Self::Explorer(_)
                | Self::DeleteSelected
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

impl Action {
    /// The name the command palette shows, written as "Group: Verb". `None`
    /// for actions that are not worth searching for: typing, single cursor
    /// steps and plugin hooks.
    pub(crate) fn title(&self) -> Option<&'static str> {
        use FocusDirection::*;
        Some(match self {
            Self::Quit => "Application: Quit",
            Self::CommandPalette => "Application: Command Palette",
            Self::FocusExplorer => "Explorer: Focus",
            Self::ToggleExplorer => "View: Toggle Explorer",
            Self::ToggleStatusBar => "View: Toggle Status Bar",
            Self::FindFile => "File: Go to File",
            Self::FindInProject => "Search: Find in Project",
            Self::FindInFile => "Search: Find in File",
            Self::Explorer(ExplorerCommand::Refresh) => "Explorer: Refresh",
            Self::DeleteSelected => "Explorer: Delete Selected",
            Self::ToggleMouse => "View: Toggle Mouse",
            Self::Save => "File: Save",
            Self::NewFile => "File: New",
            Self::CloseTab => "Tab: Close",
            Self::NextTab => "Tab: Next",
            Self::PreviousTab => "Tab: Previous",
            Self::SplitRight => "Pane: Split Right",
            Self::SplitDown => "Pane: Split Down",
            Self::FocusNextPane => "Pane: Focus Next",
            Self::FocusPane(Left) => "Pane: Focus Left",
            Self::FocusPane(Right) => "Pane: Focus Right",
            Self::FocusPane(Up) => "Pane: Focus Up",
            Self::FocusPane(Down) => "Pane: Focus Down",
            Self::Undo => "Edit: Undo",
            Self::Redo => "Edit: Redo",
            Self::Cut => "Edit: Cut",
            Self::Copy => "Edit: Copy",
            Self::Paste => "Edit: Paste",
            Self::Indent => "Edit: Indent",
            Self::Outdent => "Edit: Outdent",
            Self::DeleteWordBackward => "Edit: Delete Word Backward",
            Self::DeleteWordForward => "Edit: Delete Word Forward",
            Self::SelectAll => "Selection: Select All",
            Self::Move(Motion::DocumentStart) => "Cursor: Go to Document Start",
            Self::Move(Motion::DocumentEnd) => "Cursor: Go to Document End",
            Self::Explorer(_) => return None,
            Self::TogglePluginView(_) => return None,
            Self::Move(_)
            | Self::Select(_)
            | Self::InsertText(_)
            | Self::InsertNewline
            | Self::DeleteBackward
            | Self::DeleteForward
            | Self::Plugin(_) => return None,
        })
    }

    /// Every action the command palette lists. Actions with an argument appear
    /// once per useful value.
    pub(crate) fn palette_actions() -> Vec<Self> {
        use FocusDirection::*;
        vec![
            Self::CommandPalette,
            Self::FindFile,
            Self::FindInProject,
            Self::FindInFile,
            Self::FocusExplorer,
            Self::ToggleExplorer,
            Self::ToggleStatusBar,
            Self::Explorer(ExplorerCommand::Refresh),
            Self::DeleteSelected,
            Self::Save,
            Self::NewFile,
            Self::CloseTab,
            Self::NextTab,
            Self::PreviousTab,
            Self::SplitRight,
            Self::SplitDown,
            Self::FocusNextPane,
            Self::FocusPane(Left),
            Self::FocusPane(Right),
            Self::FocusPane(Up),
            Self::FocusPane(Down),
            Self::Undo,
            Self::Redo,
            Self::Cut,
            Self::Copy,
            Self::Paste,
            Self::SelectAll,
            Self::Indent,
            Self::Outdent,
            Self::DeleteWordBackward,
            Self::DeleteWordForward,
            Self::Move(Motion::DocumentStart),
            Self::Move(Motion::DocumentEnd),
            Self::ToggleMouse,
            Self::Quit,
        ]
    }
}
