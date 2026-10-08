use std::collections::HashMap;
use std::time::Instant;

use crate::components::PluginViewId;
use crate::components::editor::{Editor, EditorRef};
use crate::components::explorer::{Explorer, ExplorerError};
use crate::components::notifications::{Level, Notifications};
use crate::components::palette::Palette;
use crate::components::quit_prompt::QuitPrompt;
use crate::components::status_view::StatusBar;
use crate::components::workspace::Workspace;
use crate::ui::layout::LayoutTree;
use crate::ui::theme::{Rgb, Theme};
use crate::ui::view::ViewNode;

/// Which component the keyboard goes to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Focus {
    #[default]
    Editor,
    Explorer,
}

/// Everything the UI renders from. Owned by the app loop alone.
#[derive(Debug, Default)]
pub(crate) struct AppState {
    components: AppComponents,
    theme: Theme,
    layout: LayoutTree,
    notifications: Notifications,
    focus: Focus,
    pending_keys: Option<Box<str>>,
    quit_prompt: Option<QuitPrompt>,
    palette: Option<Palette>,
}

#[derive(Debug, Default)]
struct AppComponents {
    workspace: Workspace,
    explorer: Explorer,
    status_bar: StatusBar,
    plugin_views: HashMap<PluginViewId, PluginView>,
}

/// Content owned by a plugin. `version` increases on every update so stale
/// writes from a plugin can be detected.
#[derive(Debug, Clone, PartialEq, Eq)]
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

    pub(crate) fn status_bar(&self) -> &StatusBar {
        &self.components.status_bar
    }

    pub(crate) fn status_bar_mut(&mut self) -> &mut StatusBar {
        &mut self.components.status_bar
    }

    pub(crate) fn explorer(&self) -> &Explorer {
        &self.components.explorer
    }

    pub(crate) fn explorer_mut(&mut self) -> &mut Explorer {
        &mut self.components.explorer
    }

    /// Shows the project in `root` in the explorer.
    pub(crate) fn open_project(&mut self, root: &std::path::Path) -> Result<(), ExplorerError> {
        self.components.explorer = Explorer::open(root)?;
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

    /// The first chords of a key sequence that is waiting for more, shown in
    /// the status bar so it is clear the editor is waiting.
    pub(crate) fn pending_keys(&self) -> Option<&str> {
        self.pending_keys.as_deref()
    }

    pub(crate) fn set_pending_keys(&mut self, keys: Option<String>) {
        self.pending_keys = keys.map(String::into_boxed_str);
    }

    /// The unsaved-changes prompt, while it is open. It takes all keys.
    pub(crate) fn quit_prompt(&self) -> Option<&QuitPrompt> {
        self.quit_prompt.as_ref()
    }

    pub(crate) fn open_quit_prompt(&mut self, prompt: QuitPrompt) {
        self.quit_prompt = Some(prompt);
    }

    pub(crate) fn take_quit_prompt(&mut self) -> Option<QuitPrompt> {
        self.quit_prompt.take()
    }

    /// The command palette, while it is open. It takes all keys.
    pub(crate) fn palette(&self) -> Option<&Palette> {
        self.palette.as_ref()
    }

    pub(crate) fn open_palette(&mut self, palette: Palette) {
        self.palette = Some(palette);
    }

    pub(crate) fn take_palette(&mut self) -> Option<Palette> {
        self.palette.take()
    }

    #[cfg(test)]
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
        self.components.plugin_views.get(id)
    }

    pub(crate) fn set_plugin_view(&mut self, id: PluginViewId, content: ViewNode) {
        let version = self
            .components
            .plugin_views
            .get(&id)
            .map_or(0, |existing| existing.version + 1);

        self.components
            .plugin_views
            .insert(id, PluginView { version, content });
    }
}
