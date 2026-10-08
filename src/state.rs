use std::collections::HashMap;

use crate::components::status_view::StatusBar;
use crate::components::{PluginViewId, editor::Editor};
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
}

#[derive(Debug, Default)]
struct AppComponents {
    editor: Editor,
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
                editor,
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

    pub(crate) fn editor(&self) -> &Editor {
        &self.components.editor
    }

    pub(crate) fn editor_mut(&mut self) -> &mut Editor {
        &mut self.components.editor
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
