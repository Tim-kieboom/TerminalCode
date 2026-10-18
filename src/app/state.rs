use std::collections::HashMap;

use crate::components::completion_box::CompletionBox;
use crate::components::editor::{Editor, EditorRef};
use crate::components::errors::ErrorsView;
use crate::components::explorer::Explorer;
use crate::components::hover_box::HoverBox;
use crate::components::menu_bar::MenuBar;
use crate::components::notifications::{Hit, Notifications};
use crate::components::popups::action_list::ActionList;
use crate::components::popups::confirm::Confirm;
use crate::components::popups::definition_list::DefinitionList;
use crate::components::popups::diagnostic_popup::DiagnosticPopup;
use crate::components::popups::find::Find;
use crate::components::popups::finder::Finder;
use crate::components::popups::menu::Menu;
use crate::components::popups::name_prompt::NamePrompt;
use crate::components::popups::palette::Palette;
use crate::components::popups::plugin_list::PluginList;
use crate::components::popups::quit_prompt::QuitPrompt;
use crate::components::popups::root_prompt::RootPrompt;
use crate::components::popups::search::Search;
use crate::components::status_view::StatusBar;
use crate::components::terminal::TerminalPane;
use crate::components::workspace::Workspace;
use crate::event::action::Action;
use crate::persist::saved_state::Place;
use crate::syntax::SyntaxWorker;
use crate::ui::ComponentKind;
use crate::ui::Hideable;
use crate::ui::PluginViewId;
use crate::ui::layout::LayoutTree;
use crate::ui::theme::{Rgb, Theme};
use crate::ui::view::ViewNode;

mod components;
mod git;
mod lsp;
mod notices;
mod plugins;
mod popups;

/// How many palette actions are remembered as recent.
const RECENT_PALETTE_ACTIONS: usize = 8;

/// Which component the keyboard goes to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Focus {
    #[default]
    Editor,
    Explorer,
    Terminal,
    Errors,
    /// A plugin's pane (see `AppState::focused_plugin`).
    Plugin,
}

/// Everything the UI renders from. Owned by the app loop alone.
#[derive(Debug, Default)]
pub(crate) struct AppState {
    theme: Theme,
    focus: Focus,
    popup: Popup,
    /// The layout as the file says it, and `layout` is that with the views of
    /// plugins the user placed added (see `plugin_places`).
    base_layout: LayoutTree,
    layout: LayoutTree,
    /// Where the user put the plugins that run, for `layout` and the popup.
    plugin_places: Vec<(PluginViewId, Place)>,
    /// The view each bar shows, as an index into the bar's views; a bar is
    /// named by its first view, and one that is not here shows its first.
    bar_views: HashMap<ComponentKind, usize>,
    components: AppComponents,
    notifications: Notifications,
    /// The actions run from the palette lately, the latest first.
    palette_recent: Vec<Action>,
    /// Where the notifications were drawn last, for clicks on them.
    notification_hits: Vec<(Hit, ratatui::layout::Rect)>,
    /// The size of the screen as of the last draw, for working out where
    /// something that is not drawn yet would go.
    screen: ratatui::layout::Rect,
    /// `ctrl+b` was pressed in the terminal and the key after it is awaited.
    terminal_prefix: bool,
    pending_keys: Option<String>,
    /// How the language server of the file in front is doing, for the status bar.
    lsp_status: Option<String>,
    /// Whether a format, or a save that formats first, is waiting for a server.
    formatting: bool,
    /// The branch and the changed files, once git has been asked.
    git: Option<crate::git::Status>,
    /// What shows beside each changed file, from `git`.
    git_marks: crate::git::Marks,
    /// The plugin pane that has the keyboard while the focus is on a plugin.
    focused_plugin: Option<PluginViewId>,
    /// Whether the keys go to the focused plugin as text: its view is a line to
    /// type into and `esc` has not ended that.
    plugin_input: bool,
    /// Where each plugin pane was drawn last, for clicks.
    plugin_areas: HashMap<PluginViewId, (ratatui::layout::Rect, ratatui::layout::Rect)>,
    /// What the language servers say is wrong with the open documents.
    lsp_diagnostics: crate::lsp::Diagnostics,
    /// The suggestions for the word being typed, while there are any.
    completion_box: Option<CompletionBox>,
    /// What a language server says about the thing under the pointer or the cursor.
    hover_box: Option<HoverBox>,
    /// The thread that parses for syntax highlighting, once the app has an
    /// event channel for it to answer on.
    syntax: Option<SyntaxWorker>,
}

/// The overlay that has the keyboard, if any. There is only ever one: opening
/// a popup closes the one that was open.
#[derive(Debug, Default)]
pub(crate) enum Popup {
    #[default]
    None,
    QuitPrompt(QuitPrompt),
    Confirm(Confirm),
    NewEntry(NamePrompt),
    Menu(Menu),
    Diagnostics(DiagnosticPopup),
    Definitions(DefinitionList),
    Actions(ActionList),
    RootPassword(RootPrompt),
    Palette(Palette),
    Finder(Finder),
    Search(Search),
    Find(Find),
    /// The view of a plugin the user put in a popup.
    PluginView(PluginViewId),
    /// The plugin manager.
    PluginList(PluginList),
}

/// The parts of the screen the layout places. The explorer, the status bar and
/// plugin views can be hidden; the editor cannot.
#[derive(Debug)]
struct AppComponents {
    workspace: Workspace,
    explorer: Hideable<Explorer>,
    status_bar: Hideable<StatusBar>,
    terminal: Hideable<TerminalPane>,
    menu_bar: Hideable<MenuBar>,
    errors: Hideable<ErrorsView>,
    plugin_views: HashMap<PluginViewId, Hideable<PluginView>>,
}

impl Default for AppComponents {
    // Not derived: the explorer and the status bar start shown.
    fn default() -> Self {
        Self {
            workspace: Workspace::default(),
            explorer: Hideable::new_show(Explorer::default()),
            status_bar: Hideable::new_show(StatusBar),
            terminal: Hideable::new_hidden(TerminalPane::default()),
            menu_bar: Hideable::new_show(MenuBar),
            errors: Hideable::new_hidden(ErrorsView::default()),
            plugin_views: HashMap::new(),
        }
    }
}

/// Content owned by a plugin. `version` increases on every update so stale
/// writes from a plugin can be detected.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PluginView {
    version: u64,
    content: ViewNode,
}

impl PluginView {
    #[cfg(test)]
    pub(crate) fn version(&self) -> u64 {
        self.version
    }

    pub(crate) fn content(&self) -> &ViewNode {
        &self.content
    }
}

impl AppState {
    pub(crate) fn new(editor: Editor) -> Self {
        Self {
            components: AppComponents {
                workspace: Workspace::new(editor),
                ..Default::default()
            },
            ..Self::default()
        }
    }

    pub(crate) fn set_terminal_prefix(&mut self, pending: bool) {
        self.terminal_prefix = pending;
    }

    pub(crate) fn terminal_prefix_pending(&self) -> bool {
        self.terminal_prefix
    }

    /// Sets the keys of a half-typed sequence the status bar shows; says whether that changed anything.
    pub(crate) fn set_pending_keys(&mut self, keys: Option<String>) -> bool {
        let changed = self.pending_keys != keys;
        self.pending_keys = keys;
        changed
    }

    pub(crate) fn pending_keys(&self) -> Option<&str> {
        self.pending_keys.as_deref()
    }

    pub(crate) fn set_screen(&mut self, screen: ratatui::layout::Rect) {
        self.screen = screen;
    }

    pub(crate) fn focus(&self) -> Focus {
        self.focus
    }

    /// Whether typing goes to `focus` right now: it has the focus and no popup
    /// has taken the keys. The completion box is not a popup, the editor keeps
    /// the keyboard while it is open.
    pub(crate) fn has_keyboard(&self, focus: Focus) -> bool {
        self.focus == focus && matches!(self.popup, Popup::None)
    }

    pub(crate) fn set_focus(&mut self, focus: Focus) {
        self.focus = focus;
    }

    pub(crate) fn workspace(&self) -> &Workspace {
        &self.components.workspace
    }

    pub(crate) fn workspace_mut(&mut self) -> &mut Workspace {
        &mut self.components.workspace
    }

    /// Sets the syntax worker, so documents get colors.
    pub(crate) fn set_syntax_worker(&mut self, worker: SyntaxWorker) {
        self.syntax = Some(worker);
    }

    /// A copy of the theme for the syntax worker, which lives on another thread.
    pub(crate) fn theme_for_worker(&self) -> Theme {
        self.theme.clone()
    }

    /// Asks the syntax worker for the colors of everything on screen; returns
    /// what went wrong for documents it could not highlight, each once.
    pub(crate) fn refresh_highlights(&mut self) -> Vec<String> {
        self.components
            .workspace
            .refresh_highlights(self.syntax.as_ref())
    }

    /// The focused editor, for reading.
    pub(crate) fn editor(&self) -> EditorRef<'_> {
        self.components.workspace.active_editor()
    }

    /// Runs `f` on the focused editor; see [`Workspace::with_editor`].
    pub(crate) fn edit<R>(&mut self, f: impl FnOnce(&mut Editor) -> R) -> R {
        self.components.workspace.with_editor(f)
    }

    /// Gives the theme the terminal's own background color if the theme
    /// blends with it. `query` only runs in that case, since asking the
    /// terminal takes a round trip.
    pub(crate) fn learn_terminal_background(&mut self, query: impl FnOnce() -> Option<Rgb>) {
        if self.theme.needs_terminal_background() {
            self.theme.set_terminal_background(query());
        }
    }

    pub(crate) fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    pub(crate) fn theme(&self) -> &Theme {
        &self.theme
    }

    /// The screen as of the last draw.
    pub(crate) fn screen(&self) -> ratatui::layout::Rect {
        self.screen
    }

    /// Notes that the palette ran `action`, so it is offered first next time.
    pub(crate) fn remember_palette_action(&mut self, action: Action) {
        self.palette_recent.retain(|known| *known != action);
        self.palette_recent.insert(0, action);
        self.palette_recent.truncate(RECENT_PALETTE_ACTIONS);
    }

    /// The actions the palette ran lately, the latest first.
    pub(crate) fn palette_recent(&self) -> &[Action] {
        &self.palette_recent
    }
}
