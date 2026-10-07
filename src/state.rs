use std::collections::HashMap;

use crate::component::PluginViewId;
use crate::ui::layout::LayoutTree;
use crate::ui::theme::Theme;
use crate::ui::view::ViewNode;

/// Everything the UI renders from. Owned by the app loop alone.
#[derive(Debug, Default)]
pub(crate) struct AppState {
    layout: LayoutTree,
    theme: Theme,
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
    pub(crate) fn content(&self) -> &ViewNode {
        &self.content
    }
}

impl AppState {
    pub(crate) fn layout(&self) -> &LayoutTree {
        &self.layout
    }

    pub(crate) fn theme(&self) -> &Theme {
        &self.theme
    }

    pub(crate) fn plugin_view(&self, id: &PluginViewId) -> Option<&PluginView> {
        self.plugin_views.get(id)
    }

    pub(crate) fn set_plugin_view(&mut self, id: PluginViewId, content: ViewNode) {
        let version = self
            .plugin_views
            .get(&id)
            .map_or(0, |existing| existing.version + 1);

        self.plugin_views
            .insert(id, PluginView { version, content });
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
