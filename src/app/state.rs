use std::collections::HashMap;
use std::time::Instant;

use crate::components::ComponentKind;
use crate::components::PluginViewId;
use crate::components::confirm::Confirm;
use crate::components::editor::{Editor, EditorRef};
use crate::components::explorer::{Explorer, ExplorerError};
use crate::components::find::Find;
use crate::components::finder::Finder;
use crate::components::menu::Menu;
use crate::components::name_prompt::NamePrompt;
use crate::components::notifications::{Level, Notifications};
use crate::components::palette::Palette;
use crate::components::quit_prompt::QuitPrompt;
use crate::components::search::Search;
use crate::components::status_view::StatusBar;
use crate::components::terminal::TerminalPane;
use crate::components::workspace::Workspace;
use crate::syntax::SyntaxWorker;
use crate::ui::Hideable;
use crate::ui::layout::LayoutTree;
use crate::ui::theme::{Rgb, Theme};
use crate::ui::view::ViewNode;

/// Which component the keyboard goes to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Focus {
    #[default]
    Editor,
    Explorer,
    Terminal,
}

/// Everything the UI renders from. Owned by the app loop alone.
#[derive(Debug, Default)]
pub(crate) struct AppState {
    theme: Theme,
    focus: Focus,
    popup: Popup,
    layout: LayoutTree,
    components: AppComponents,
    notifications: Notifications,
    /// The size of the screen as of the last draw, for working out where
    /// something that is not drawn yet would go.
    screen: ratatui::layout::Rect,
    /// `ctrl+b` was pressed in the terminal and the key after it is awaited.
    terminal_prefix: bool,
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
    Palette(Palette),
    Finder(Finder),
    Search(Search),
    Find(Find),
}

/// The parts of the screen the layout places. The explorer, the status bar and
/// plugin views can be hidden; the editor cannot.
#[derive(Debug)]
struct AppComponents {
    workspace: Workspace,
    explorer: Hideable<Explorer>,
    status_bar: Hideable<StatusBar>,
    terminal: Hideable<TerminalPane>,
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

    /// Whether the component takes part in the layout. Hidden ones take no
    /// space. The editor and anything this does not know are always shown.
    pub(crate) fn is_visible(&self, kind: &ComponentKind) -> bool {
        match kind {
            ComponentKind::Explorer => self.components.explorer.is_shown(),
            ComponentKind::StatusBar => self.components.status_bar.is_shown(),
            ComponentKind::Plugin(id) => self
                .components
                .plugin_views
                .get(id)
                .is_none_or(Hideable::is_shown),
            ComponentKind::Terminal => self.components.terminal.is_shown(),
            ComponentKind::Editor => true,
        }
    }

    pub(crate) fn show_explorer(&mut self) {
        self.components.explorer.show();
    }

    pub(crate) fn hide_explorer(&mut self) {
        self.components.explorer.hide();
    }

    pub(crate) fn terminal(&self) -> &TerminalPane {
        self.components.terminal.node()
    }

    pub(crate) fn terminal_mut(&mut self) -> &mut TerminalPane {
        self.components.terminal.node_mut()
    }

    pub(crate) fn set_terminal_prefix(&mut self, pending: bool) {
        self.terminal_prefix = pending;
    }

    pub(crate) fn terminal_prefix_pending(&self) -> bool {
        self.terminal_prefix
    }

    pub(crate) fn set_screen(&mut self, screen: ratatui::layout::Rect) {
        self.screen = screen;
    }

    /// Rows and columns the terminal pane has, or would have if it is shown
    /// now, going by the screen as of the last draw. `None` before the first.
    pub(crate) fn terminal_size_now(&self) -> Option<(u16, u16)> {
        let placements = self
            .layout
            .resolve_visible(self.screen, &|kind| self.is_visible(kind));
        let placement = placements
            .iter()
            .find(|placement| placement.kind == ComponentKind::Terminal)?;
        let inner = placement.frame.inner(placement.area);
        Some((inner.height, inner.width))
    }

    pub(crate) fn show_terminal(&mut self) {
        self.components.terminal.show();
    }

    pub(crate) fn hide_terminal(&mut self) {
        self.components.terminal.hide();
    }

    pub(crate) fn toggle_status_bar(&mut self) {
        self.components.status_bar.toggle();
    }

    /// Shows or hides a plugin's view. Returns whether the plugin has one.
    pub(crate) fn toggle_plugin_view(&mut self, id: &PluginViewId) -> bool {
        let Some(view) = self.components.plugin_views.get_mut(id) else {
            return false;
        };
        view.toggle();
        true
    }

    pub(crate) fn status_bar(&self) -> &StatusBar {
        self.components.status_bar.node()
    }

    pub(crate) fn status_bar_mut(&mut self) -> &mut StatusBar {
        self.components.status_bar.node_mut()
    }

    pub(crate) fn explorer(&self) -> &Explorer {
        self.components.explorer.node()
    }

    pub(crate) fn explorer_mut(&mut self) -> &mut Explorer {
        self.components.explorer.node_mut()
    }

    /// Shows the project in `root` in the explorer.
    pub(crate) fn open_project(&mut self, root: &std::path::Path) -> Result<(), ExplorerError> {
        *self.components.explorer.node_mut() = Explorer::open(root)?;
        Ok(())
    }

    pub(crate) fn focus(&self) -> Focus {
        self.focus
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

    /// Shows an info message that goes away by itself.
    pub(crate) fn notify(&mut self, message: impl Into<Box<str>>) {
        self.notifications
            .push(Level::Info, message, Instant::now());
    }

    /// Shows an error that stays until the user presses a key or clicks.
    pub(crate) fn notify_error(&mut self, message: impl Into<Box<str>>) {
        self.notifications
            .push(Level::Error, message, Instant::now());
    }

    pub(crate) fn notifications(&self) -> &Notifications {
        &self.notifications
    }

    /// The text of the newest message, if any.
    #[cfg(test)]
    pub(crate) fn latest_notification(&self) -> Option<&str> {
        self.notifications.latest().map(|item| &*item.text)
    }

    /// Removes errors the user has had a chance to see. Returns whether there
    /// were any.
    pub(crate) fn dismiss_errors(&mut self) -> bool {
        self.notifications.dismiss_errors()
    }

    /// Removes info messages that are over by `now`. Returns whether any went.
    pub(crate) fn expire_notifications(&mut self, now: Instant) -> bool {
        self.notifications.expire(now)
    }

    /// The popup that is open, which has all the keys.
    pub(crate) fn popup(&self) -> &Popup {
        &self.popup
    }

    /// The unsaved-changes prompt, while it is open.
    #[cfg(test)]
    pub(crate) fn quit_prompt(&self) -> Option<&QuitPrompt> {
        match &self.popup {
            Popup::QuitPrompt(prompt) => Some(prompt),
            _ => None,
        }
    }

    /// Opens the prompt, closing any other popup.
    pub(crate) fn open_quit_prompt(&mut self, prompt: QuitPrompt) {
        self.popup = Popup::QuitPrompt(prompt);
    }

    /// Closes the quit_prompt and hands it back; any other popup stays open.
    pub(crate) fn take_quit_prompt(&mut self) -> Option<QuitPrompt> {
        if !matches!(self.popup, Popup::QuitPrompt(_)) {
            return None;
        }
        match std::mem::take(&mut self.popup) {
            Popup::QuitPrompt(prompt) => Some(prompt),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn confirm(&self) -> Option<&Confirm> {
        match &self.popup {
            Popup::Confirm(confirm) => Some(confirm),
            _ => None,
        }
    }

    /// Opens the question, closing any other popup.
    pub(crate) fn open_confirm(&mut self, confirm: Confirm) {
        self.popup = Popup::Confirm(confirm);
    }

    /// Closes the question and hands it back; any other popup stays open.
    pub(crate) fn take_confirm(&mut self) -> Option<Confirm> {
        if !matches!(self.popup, Popup::Confirm(_)) {
            return None;
        }
        match std::mem::take(&mut self.popup) {
            Popup::Confirm(confirm) => Some(confirm),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn menu(&self) -> Option<&Menu> {
        match &self.popup {
            Popup::Menu(menu) => Some(menu),
            _ => None,
        }
    }

    /// The open context menu, to place it before it is drawn.
    pub(crate) fn menu_mut(&mut self) -> Option<&mut Menu> {
        match &mut self.popup {
            Popup::Menu(menu) => Some(menu),
            _ => None,
        }
    }

    pub(crate) fn open_menu(&mut self, menu: Menu) {
        self.popup = Popup::Menu(menu);
    }

    /// Closes the menu and hands it back; any other popup stays open.
    pub(crate) fn take_menu(&mut self) -> Option<Menu> {
        if !matches!(self.popup, Popup::Menu(_)) {
            return None;
        }
        match std::mem::take(&mut self.popup) {
            Popup::Menu(menu) => Some(menu),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn name_prompt(&self) -> Option<&NamePrompt> {
        match &self.popup {
            Popup::NewEntry(prompt) => Some(prompt),
            _ => None,
        }
    }

    pub(crate) fn open_name_prompt(&mut self, prompt: NamePrompt) {
        self.popup = Popup::NewEntry(prompt);
    }

    /// Closes the prompt and hands it back; any other popup stays open.
    pub(crate) fn take_name_prompt(&mut self) -> Option<NamePrompt> {
        if !matches!(self.popup, Popup::NewEntry(_)) {
            return None;
        }
        match std::mem::take(&mut self.popup) {
            Popup::NewEntry(prompt) => Some(prompt),
            _ => None,
        }
    }

    /// Find in the open file, while its bar is open.
    pub(crate) fn find(&self) -> Option<&Find> {
        match &self.popup {
            Popup::Find(find) => Some(find),
            _ => None,
        }
    }

    pub(crate) fn open_find(&mut self, find: Find) {
        self.popup = Popup::Find(find);
    }

    /// Closes the find and hands it back; any other popup stays open.
    pub(crate) fn take_find(&mut self) -> Option<Find> {
        if !matches!(self.popup, Popup::Find(_)) {
            return None;
        }
        match std::mem::take(&mut self.popup) {
            Popup::Find(find) => Some(find),
            _ => None,
        }
    }

    /// The project search, while it is open.
    #[cfg(test)]
    pub(crate) fn search(&self) -> Option<&Search> {
        match &self.popup {
            Popup::Search(search) => Some(search),
            _ => None,
        }
    }

    pub(crate) fn search_mut(&mut self) -> Option<&mut Search> {
        match &mut self.popup {
            Popup::Search(search) => Some(search),
            _ => None,
        }
    }

    pub(crate) fn open_search(&mut self, search: Search) {
        self.popup = Popup::Search(search);
    }

    /// Closes the search and hands it back; any other popup stays open.
    pub(crate) fn take_search(&mut self) -> Option<Search> {
        if !matches!(self.popup, Popup::Search(_)) {
            return None;
        }
        match std::mem::take(&mut self.popup) {
            Popup::Search(search) => Some(search),
            _ => None,
        }
    }

    /// The file finder, while it is open.
    #[cfg(test)]
    pub(crate) fn finder(&self) -> Option<&Finder> {
        match &self.popup {
            Popup::Finder(finder) => Some(finder),
            _ => None,
        }
    }

    pub(crate) fn finder_mut(&mut self) -> Option<&mut Finder> {
        match &mut self.popup {
            Popup::Finder(finder) => Some(finder),
            _ => None,
        }
    }

    pub(crate) fn open_finder(&mut self, finder: Finder) {
        self.popup = Popup::Finder(finder);
    }

    /// Closes the finder and hands it back; any other popup stays open.
    pub(crate) fn take_finder(&mut self) -> Option<Finder> {
        if !matches!(self.popup, Popup::Finder(_)) {
            return None;
        }
        match std::mem::take(&mut self.popup) {
            Popup::Finder(finder) => Some(finder),
            _ => None,
        }
    }

    /// The command palette, while it is open.
    #[cfg(test)]
    pub(crate) fn palette(&self) -> Option<&Palette> {
        match &self.popup {
            Popup::Palette(palette) => Some(palette),
            _ => None,
        }
    }

    pub(crate) fn open_palette(&mut self, palette: Palette) {
        self.popup = Popup::Palette(palette);
    }

    /// Closes the palette and hands it back; any other popup stays open.
    pub(crate) fn take_palette(&mut self) -> Option<Palette> {
        if !matches!(self.popup, Popup::Palette(_)) {
            return None;
        }
        match std::mem::take(&mut self.popup) {
            Popup::Palette(palette) => Some(palette),
            _ => None,
        }
    }

    pub(crate) fn set_layout(&mut self, layout: LayoutTree) {
        self.layout = layout;
    }

    pub(crate) fn layout(&self) -> &LayoutTree {
        &self.layout
    }

    /// Gives the theme the terminal's own background color if the theme
    /// blends with it. `query` only runs in that case, since asking the
    /// terminal takes a round trip.
    pub(crate) fn learn_terminal_background(&mut self, query: impl FnOnce() -> Option<Rgb>) {
        if self.theme.needs_terminal_background() {
            self.theme.set_terminal_background(query());
        }
    }

    #[cfg(test)]
    pub(crate) fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    pub(crate) fn theme(&self) -> &Theme {
        &self.theme
    }

    pub(crate) fn plugin_view(&self, id: &PluginViewId) -> Option<&PluginView> {
        self.components.plugin_views.get(id).map(|view| view.node())
    }

    pub(crate) fn set_plugin_view(&mut self, id: PluginViewId, content: ViewNode) {
        let version = self
            .components
            .plugin_views
            .get(&id)
            .map_or(0, |existing| existing.node().version + 1);
        let view = PluginView { version, content };

        // An update does not show a view the user hid.
        match self.components.plugin_views.get_mut(&id) {
            Some(existing) => *existing.node_mut() = view,
            None => {
                self.components
                    .plugin_views
                    .insert(id, Hideable::new_show(view));
            }
        }
    }
}
