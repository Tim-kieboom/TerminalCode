use serde::Deserialize;

use crate::components::editor::Motion;
use crate::components::errors::ErrorsCommand;
use crate::components::explorer::ExplorerCommand;
use crate::components::workspace::FocusDirection;
use crate::config::ConfigFile;

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
    /// Moves the keyboard to the editor.
    FocusEditor,
    /// Shows the explorer if it is hidden, hides it if it is shown.
    ToggleExplorer,
    ToggleStatusBar,
    /// Shows the terminal pane (starting the shell the first time), or hides it.
    ToggleTerminal,
    /// Opens another shell in a new tab of the terminal pane.
    TerminalNewTab,
    /// Copies the text selected in the terminal pane.
    TerminalCopy,
    TerminalNextTab,
    TerminalPreviousTab,
    /// Closes the active terminal tab and stops its shell.
    TerminalCloseTab,
    /// Shows or hides the view of the plugin with this id.
    TogglePluginView(Box<str>),
    /// Shows the view of the plugin with this id if it is hidden, and gives it the
    /// keyboard.
    FocusPluginView(Box<str>),
    /// Opens the list of the plugins the user has, to turn each on or off.
    ShowPlugins,
    /// Asks whether to trust the project folder: plugins may run commands in a folder the
    /// user trusts (the tests of the project, for one).
    TrustFolder,
    /// Opens the view of the plugin with this id in a popup (the plugins the user
    /// placed in one), or gives the plugin the keyboard where it is placed.
    #[serde(alias = "open_plugin")]
    OpenPluginPopup(Box<str>),
    /// Puts the plugin's view where the user chose; written `plugin-id:place` with the
    /// place `explorer`, `bottom` or `popup`.
    PluginPlaced(Box<str>),
    /// Turns the plugin with this id on (asking what it may do, when it has not
    /// been asked) or off.
    TogglePlugin(Box<str>),
    /// Opens the file finder.
    FindFile,
    /// Opens the project search.
    FindInProject,
    /// Goes to the next result of the last project search.
    NextSearchResult,
    /// Goes to the previous result of the last project search.
    PreviousSearchResult,
    /// Moves the cursor to the next diagnostic of the open file, from a language
    /// server, and round to the first after the last.
    NextDiagnostic,
    /// Moves the cursor to the previous diagnostic of the open file.
    PreviousDiagnostic,
    /// Shows the whole text of the diagnostics on the cursor line in a box.
    ShowDiagnostics,
    /// Shows what the language server says about the word at the cursor in a box.
    ShowHover,
    /// Goes back to the place the last jump left (the jump list).
    JumpBack,
    /// Goes forward again after going back.
    JumpForward,
    /// Goes to where the word at the cursor is defined.
    GoToDefinition,
    RenameSymbol,
    CodeActions,
    FormatDocument,
    FormatSelection,
    /// Opens the find bar for the open file.
    FindInFile,
    /// Goes to the next match of the find in the open file, from the cursor, with the bar open
    /// or not (it opens on the last query when it is not).
    FindNext,
    FindPrevious,
    /// Opens the find bar with the row for replacing.
    ReplaceInFile,
    /// Deletes the explorer's selected file or folder, to the trash.
    DeleteSelected,
    /// Asks for the name of a new file in the explorer's selected folder.
    CreateFile,
    /// Asks for the name of a new folder in the explorer's selected folder.
    CreateFolder,
    /// Asks for a new name for the explorer's selected file or folder.
    Rename,
    /// Asks where to move the explorer's selected file or folder to.
    MoveEntry,
    /// Opens the context menu on the explorer's selected row.
    ContextMenu,
    /// Asks for a line of the open file to go to.
    GoToLine,
    /// Shows the file of the active tab in the explorer and selects it.
    RevealActiveFile,
    /// Reads the theme, layout and keymap files again and applies them.
    Reload,
    /// Switches to the installed theme with this name (a file in the themes folder).
    UseInstalledTheme(Box<str>),
    /// Lists the installed themes to choose from.
    ChooseTheme,
    /// Asks which theme to start from, then for a name, and makes the new theme.
    NewTheme,
    /// Asks for the name of a new theme that starts as a copy of this one (`Dark`,
    /// `Light`, or an installed theme).
    NewThemeFrom(Box<str>),
    /// Opens one of the user's config files, first making it from the built-in
    /// default if there is none yet.
    OpenConfig(ConfigFile),
    /// Asks, then puts the built-in default in place of one of the user's config
    /// files; the old file is kept beside it as `NAME.bak`.
    ResetConfig(ConfigFile),
    /// Asks, then forgets which panes were hidden or shown, with the old
    /// `state.json` kept as `state.json.bak`.
    ResetSavedState,
    Explorer(ExplorerCommand),
    /// Moves in the Errors view, or opens the problem it is on.
    Errors(ErrorsCommand),
    /// Shows the Errors view and gives it the keyboard; with the keyboard in it,
    /// hides it.
    ShowErrors,
    /// Says which language server the file in front has, or why it has none.
    ShowLspServer,
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
    /// Opens one of the files in the main menu's Open Recent.
    OpenRecentFile(Box<str>),
    /// Opens one of the folders in Open Recent, by starting the editor again there.
    OpenRecentFolder(Box<str>),
    /// Forgets the files and folders Open Recent lists.
    ClearRecent,
    /// Asks for the path of a file and opens it as a tab.
    OpenFile,
    /// Asks for the path of a folder and starts the editor again there.
    OpenFolder,
    /// Opens the main menu, under the icon in the top row (at the top left when
    /// the row is hidden).
    OpenMenu,
    /// Shows or hides the row at the top with the menu icon.
    ToggleMenuBar,
    /// Shows the next view of the bar that has the keyboard (the sidebar or the
    /// bottom bar), giving it the keyboard.
    NextBarView,
    /// Shows the previous view of the bar that has the keyboard.
    PreviousBarView,
    /// Swaps the line the cursor is on (or the selected lines) with the one above.
    MoveLineUp,
    /// Swaps the line the cursor is on (or the selected lines) with the one below.
    MoveLineDown,
    /// Adds a cursor on the line above the primary one.
    AddCursorAbove,
    /// Adds a cursor on the line below the primary one.
    AddCursorBelow,
    /// Goes back to the primary cursor alone.
    CollapseCursors,
    Indent,
    Outdent,
    /// Moves the lines the cursors are on one indent level in.
    IndentLine,
    /// Moves the lines the cursors are on one indent level out.
    OutdentLine,
    /// Comments the lines the cursors are on, or uncomments them when all are.
    ToggleComment,
    Copy,
    Cut,
    Paste,
    ToggleMouse,
    NewFile,
    CloseTab,
    /// Closes every other tab of the focused pane.
    CloseOtherTabs,
    /// Closes every tab of the focused pane to the left of the one showing.
    CloseTabsToLeft,
    /// Closes every tab of the focused pane to the right of the one showing.
    CloseTabsToRight,
    /// Pins the tab showing to the left of the strip, or lets it go.
    TogglePinTab,
    /// Lets a file that is read-only to the user be edited, to save it as root.
    UnlockForEditing,
    /// Closes every tab of every pane.
    CloseAllTabs,
    /// Opens the file of the tab closed last, with the cursor where it was.
    ReopenClosedTab,
    /// Saves every modified file.
    SaveAll,
    /// Builds and starts the active run configuration in its terminal tab.
    Run,
    /// Opens the picker of run configurations; the one chosen is run, and becomes the one
    /// the Run key starts.
    ChooseRun,
    /// Sets a breakpoint on the line of the cursor, or takes the one there away.
    ToggleBreakpoint,
    /// Takes away every breakpoint.
    ClearBreakpoints,
    NextTab,
    PreviousTab,
    SplitRight,
    SplitDown,
    /// Shows the pane right of the editor (a plugin's preview) if it is hidden, hides it if
    /// it is shown.
    ToggleSidePane,
    /// Gives the explorer's and the terminal's bars the sizes of the layout file again.
    ResetPaneSizes,
    /// Gives the pane right of the editor (a plugin's preview) more of the width.
    SidePaneWider,
    SidePaneNarrower,
    /// Gives the focused editor pane more of its split, or less.
    PaneGrow,
    PaneShrink,
    FocusNextPane,
    FocusPane(FocusDirection),
    Move(Motion),
    Select(Motion),
    /// Runs an action of a plugin, written `plugin:action` (the plugin's name, and
    /// the `id` of the action in its manifest).
    Plugin(Box<str>),
}

impl Action {
    /// Whether running the action puts the keyboard back in the editor: the
    /// ones that open, close or switch tabs and panes.
    pub(crate) fn focuses_editor(&self) -> bool {
        matches!(
            self,
            Self::NewFile
                | Self::FocusEditor
                | Self::CloseOtherTabs
                | Self::CloseTabsToLeft
                | Self::CloseTabsToRight
                | Self::TogglePinTab
                | Self::CloseAllTabs
                | Self::ReopenClosedTab
                | Self::OpenConfig(_)
                | Self::ToggleTerminal
                | Self::TerminalNewTab
                | Self::TerminalNextTab
                | Self::TerminalPreviousTab
                | Self::TerminalCloseTab
                | Self::CloseTab
                | Self::NextTab
                | Self::PreviousTab
                | Self::SplitRight
                | Self::SplitDown
                | Self::FocusNextPane
        )
    }

    /// Whether the action works on an open file, and so has nothing to do
    /// when every tab is closed.
    pub(crate) fn needs_editor(&self) -> bool {
        !matches!(
            self,
            Self::Quit
                | Self::CommandPalette
                | Self::TerminalCopy
                | Self::FocusExplorer
                | Self::FocusEditor
                | Self::FocusPluginView(_)
                | Self::ShowPlugins
                | Self::TrustFolder
                | Self::Run
                | Self::ChooseRun
                | Self::ClearBreakpoints
                | Self::OpenPluginPopup(_)
                | Self::PluginPlaced(_)
                | Self::TogglePlugin(_)
                | Self::ToggleExplorer
                | Self::ToggleStatusBar
                | Self::TogglePluginView(_)
                | Self::FindFile
                | Self::FindInProject
                | Self::NextSearchResult
                | Self::PreviousSearchResult
                | Self::Explorer(_)
                | Self::Errors(_)
                | Self::ShowErrors
                | Self::OpenMenu
                | Self::OpenFile
                | Self::OpenRecentFile(_)
                | Self::OpenRecentFolder(_)
                | Self::ClearRecent
                | Self::OpenFolder
                | Self::ToggleMenuBar
                | Self::DeleteSelected
                | Self::CreateFile
                | Self::CreateFolder
                | Self::Rename
                | Self::MoveEntry
                | Self::OpenConfig(_)
                | Self::ResetConfig(_)
                | Self::ResetSavedState
                | Self::Reload
                | Self::UseInstalledTheme(_)
                | Self::ChooseTheme
                | Self::NewTheme
                | Self::NewThemeFrom(_)
                | Self::ToggleMouse
                | Self::NewFile
                | Self::CloseOtherTabs
                | Self::CloseTabsToLeft
                | Self::CloseTabsToRight
                | Self::TogglePinTab
                | Self::CloseAllTabs
                | Self::ReopenClosedTab
                | Self::SaveAll
                | Self::CloseTab
                | Self::NextTab
                | Self::PreviousTab
                | Self::SplitRight
                | Self::SplitDown
                | Self::FocusNextPane
                | Self::ToggleSidePane
                | Self::ResetPaneSizes
                | Self::SidePaneWider
                | Self::SidePaneNarrower
                | Self::PaneGrow
                | Self::PaneShrink
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
            Self::FocusEditor => "Editor: Focus",
            Self::ShowPlugins => "Plugins: Manage",
            Self::TrustFolder => "Plugins: Trust This Folder",
            Self::ToggleExplorer => "View: Toggle Explorer",
            Self::ToggleStatusBar => "View: Toggle Status Bar",
            Self::ToggleTerminal => "View: Toggle Terminal",
            Self::TerminalNewTab => "Terminal: New Tab",
            Self::TerminalCopy => "Terminal: Copy Selection",
            Self::TerminalNextTab => "Terminal: Next Tab",
            Self::TerminalPreviousTab => "Terminal: Previous Tab",
            Self::TerminalCloseTab => "Terminal: Close Tab",
            Self::FindFile => "File: Go to File",
            Self::FindInProject => "Search: Find in Project",
            Self::NextSearchResult => "Search: Next Result",
            Self::PreviousSearchResult => "Search: Previous Result",
            Self::NextDiagnostic => "Diagnostics: Next",
            Self::PreviousDiagnostic => "Diagnostics: Previous",
            Self::ShowDiagnostics => "Diagnostics: Show at Cursor",
            Self::ShowHover => "Hover: Show Documentation at Cursor",
            Self::JumpBack => "Navigate: Back",
            Self::JumpForward => "Navigate: Forward",
            Self::GoToDefinition => "Navigate: Go to Definition",
            Self::RenameSymbol => "Language: Rename Symbol",
            Self::CodeActions => "Language: Code Action",
            Self::FormatDocument => "Language: Format Document",
            Self::FormatSelection => "Language: Format Selection",
            Self::FindInFile => "Search: Find in File",
            Self::FindNext => "Search: Find Next",
            Self::FindPrevious => "Search: Find Previous",
            Self::ReplaceInFile => "Search: Replace in File",
            Self::Explorer(ExplorerCommand::Refresh) => "Explorer: Refresh",
            Self::DeleteSelected => "Explorer: Delete Selected",
            Self::CreateFile => "Explorer: New File",
            Self::CreateFolder => "Explorer: New Folder",
            Self::Rename => "Explorer: Rename",
            Self::MoveEntry => "Explorer: Move",
            Self::ContextMenu => "Explorer: Context Menu",
            Self::GoToLine => "Cursor: Go to Line",
            Self::RevealActiveFile => "Explorer: Reveal Active File",
            Self::Explorer(ExplorerCommand::ToggleHidden) => "Explorer: Toggle Hidden Files",
            Self::OpenConfig(ConfigFile::Theme) => "Config: Open Theme",
            Self::OpenConfig(ConfigFile::Keymap) => "Config: Open Keymap",
            Self::OpenConfig(ConfigFile::Layout) => "Config: Open Layout",
            Self::OpenConfig(ConfigFile::Settings) => "Config: Open Settings",
            Self::ResetConfig(ConfigFile::Theme) => "Config: Reset Theme to Default",
            Self::ResetConfig(ConfigFile::Keymap) => "Config: Reset Keymap to Default",
            Self::ResetConfig(ConfigFile::Layout) => "Config: Reset Layout to Default",
            Self::ResetConfig(ConfigFile::Settings) => "Config: Reset Settings to Default",
            Self::ResetSavedState => "Config: Forget Hidden and Shown Panes",
            Self::Reload => "Config: Reload",
            Self::ChooseTheme => "Theme: Choose...",
            Self::NewTheme => "Theme: New...",
            Self::NewThemeFrom(_) => return None,
            Self::UseInstalledTheme(_) => return None,
            Self::ToggleMouse => "View: Toggle Mouse",
            Self::Save => "File: Save",
            Self::SaveAll => "File: Save All",
            Self::Run => "Run: Run",
            Self::ChooseRun => "Run: Choose Configuration...",
            Self::ToggleBreakpoint => "Debug: Toggle Breakpoint",
            Self::ClearBreakpoints => "Debug: Remove All Breakpoints",
            Self::CloseOtherTabs => "Tab: Close Others",
            Self::CloseTabsToLeft => "Tab: Close to the Left",
            Self::CloseTabsToRight => "Tab: Close to the Right",
            Self::TogglePinTab => "Tab: Pin or Unpin",
            Self::UnlockForEditing => "File: Edit Anyway (save as root)",
            Self::CloseAllTabs => "Tab: Close All",
            Self::ReopenClosedTab => "Tab: Reopen Closed",
            Self::NewFile => "File: New",
            Self::CloseTab => "Tab: Close",
            Self::NextTab => "Tab: Next",
            Self::PreviousTab => "Tab: Previous",
            Self::SplitRight => "Pane: Split Right",
            Self::SplitDown => "Pane: Split Down",
            Self::ToggleSidePane => "View: Toggle Side Pane",
            Self::ResetPaneSizes => "View: Reset Pane Sizes",
            Self::SidePaneWider => "View: Wider Side Pane",
            Self::SidePaneNarrower => "View: Narrower Side Pane",
            Self::PaneGrow => "Pane: Grow",
            Self::PaneShrink => "Pane: Shrink",
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
            Self::IndentLine => "Edit: Indent Line",
            Self::OutdentLine => "Edit: Outdent Line",
            Self::ToggleComment => "Edit: Toggle Line Comment",
            Self::DeleteWordBackward => "Edit: Delete Word Backward",
            Self::DeleteWordForward => "Edit: Delete Word Forward",
            Self::SelectAll => "Selection: Select All",
            Self::ShowErrors => "View: Show Errors",
            Self::ShowLspServer => "LSP: Show Server of This File",
            Self::ClearRecent => "File: Clear Recent",
            Self::OpenFile => "File: Open File...",
            Self::OpenFolder => "File: Open Folder...",
            Self::OpenMenu => "View: Open Menu",
            Self::ToggleMenuBar => "View: Toggle Menu Bar",
            Self::NextBarView => "Bar: Next View",
            Self::PreviousBarView => "Bar: Previous View",
            Self::MoveLineUp => "Edit: Move Line Up",
            Self::MoveLineDown => "Edit: Move Line Down",
            Self::AddCursorAbove => "Cursor: Add Cursor Above",
            Self::AddCursorBelow => "Cursor: Add Cursor Below",
            Self::CollapseCursors => "Cursor: Back to One Cursor",
            Self::Move(Motion::DocumentStart) => "Cursor: Go to Document Start",
            Self::Move(Motion::DocumentEnd) => "Cursor: Go to Document End",
            Self::Explorer(_) | Self::Errors(_) => return None,
            Self::OpenRecentFile(_) | Self::OpenRecentFolder(_) => return None,
            Self::TogglePluginView(_)
            | Self::FocusPluginView(_)
            | Self::OpenPluginPopup(_)
            | Self::PluginPlaced(_)
            | Self::TogglePlugin(_) => {
                return None;
            }
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
            Self::NextSearchResult,
            Self::PreviousSearchResult,
            Self::NextDiagnostic,
            Self::PreviousDiagnostic,
            Self::ShowDiagnostics,
            Self::ShowHover,
            Self::JumpBack,
            Self::JumpForward,
            Self::GoToDefinition,
            Self::RenameSymbol,
            Self::CodeActions,
            Self::FormatDocument,
            Self::FormatSelection,
            Self::FindInFile,
            Self::FindNext,
            Self::FindPrevious,
            Self::ReplaceInFile,
            Self::FocusExplorer,
            Self::FocusEditor,
            Self::ShowPlugins,
            Self::TrustFolder,
            Self::ToggleExplorer,
            Self::ToggleStatusBar,
            Self::ToggleTerminal,
            Self::TerminalNewTab,
            Self::TerminalCopy,
            Self::TerminalNextTab,
            Self::TerminalPreviousTab,
            Self::TerminalCloseTab,
            Self::Explorer(ExplorerCommand::Refresh),
            Self::DeleteSelected,
            Self::CreateFile,
            Self::CreateFolder,
            Self::Rename,
            Self::MoveEntry,
            Self::ContextMenu,
            Self::GoToLine,
            Self::RevealActiveFile,
            Self::Explorer(ExplorerCommand::ToggleHidden),
            Self::OpenConfig(ConfigFile::Theme),
            Self::OpenConfig(ConfigFile::Keymap),
            Self::OpenConfig(ConfigFile::Layout),
            Self::OpenConfig(ConfigFile::Settings),
            Self::ResetConfig(ConfigFile::Theme),
            Self::ResetConfig(ConfigFile::Keymap),
            Self::ResetConfig(ConfigFile::Layout),
            Self::ResetConfig(ConfigFile::Settings),
            Self::ResetSavedState,
            Self::Reload,
            Self::ChooseTheme,
            Self::NewTheme,
            Self::Save,
            Self::SaveAll,
            Self::Run,
            Self::ChooseRun,
            Self::ToggleBreakpoint,
            Self::ClearBreakpoints,
            Self::CloseOtherTabs,
            Self::CloseTabsToLeft,
            Self::CloseTabsToRight,
            Self::TogglePinTab,
            Self::UnlockForEditing,
            Self::CloseAllTabs,
            Self::ReopenClosedTab,
            Self::NewFile,
            Self::CloseTab,
            Self::NextTab,
            Self::PreviousTab,
            Self::SplitRight,
            Self::SplitDown,
            Self::ToggleSidePane,
            Self::ResetPaneSizes,
            Self::SidePaneWider,
            Self::SidePaneNarrower,
            Self::PaneGrow,
            Self::PaneShrink,
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
            Self::OpenFile,
            Self::OpenFolder,
            Self::ClearRecent,
            Self::OpenMenu,
            Self::ToggleMenuBar,
            Self::ShowErrors,
            Self::ShowLspServer,
            Self::NextBarView,
            Self::PreviousBarView,
            Self::MoveLineUp,
            Self::MoveLineDown,
            Self::AddCursorAbove,
            Self::AddCursorBelow,
            Self::CollapseCursors,
            Self::Indent,
            Self::Outdent,
            Self::IndentLine,
            Self::OutdentLine,
            Self::ToggleComment,
            Self::DeleteWordBackward,
            Self::DeleteWordForward,
            Self::Move(Motion::DocumentStart),
            Self::Move(Motion::DocumentEnd),
            Self::ToggleMouse,
            Self::Quit,
        ]
    }
}
