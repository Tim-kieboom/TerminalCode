use std::collections::HashMap;

use crate::components::PluginViewId;
use crate::components::editor::{Editor, EditorRef};
use crate::components::status_view::StatusBar;
use crate::components::workspace::Workspace;
use crate::ui::layout::LayoutTree;
use crate::ui::theme::Theme;
use crate::ui::view::ViewNode;

/// Everything the UI renders from. Owned by the app loop alone.
#[derive(Debug, Default)]
pub(crate) struct AppState {
    components: AppComponents,
    theme: Theme,
    layout: LayoutTree,
    status: Option<Box<str>>,
    pending_keys: Option<Box<str>>,
}

#[derive(Debug, Default)]
struct AppComponents {
    workspace: Workspace,
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

    /// One-line message for the status bar, such as an error or "saved".
    pub(crate) fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub(crate) fn set_status(&mut self, message: impl Into<Box<str>>) {
        self.status = Some(message.into());
    }

    pub(crate) fn clear_status(&mut self) {
        self.status = None;
    }

    /// The first chords of a key sequence that is waiting for more, shown in
    /// the status bar so it is clear the editor is waiting.
    pub(crate) fn pending_keys(&self) -> Option<&str> {
        self.pending_keys.as_deref()
    }

    pub(crate) fn set_pending_keys(&mut self, keys: Option<String>) {
        self.pending_keys = keys.map(String::into_boxed_str);
    }

    pub(crate) fn layout(&self) -> &LayoutTree {
        &self.layout
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
