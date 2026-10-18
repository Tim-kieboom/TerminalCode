//! Which plugin pane has the keyboard, and whether its keys are text.

use ratatui::layout::Rect;

use super::{AppState, Focus};
use crate::ui::PluginViewId;

impl AppState {
    /// The plugin pane that has the keyboard, when the focus is on a plugin.
    pub(crate) fn focused_plugin(&self) -> Option<&PluginViewId> {
        match self.focus {
            Focus::Plugin => self.focused_plugin.as_ref(),
            _ => None,
        }
    }

    /// Gives the keyboard to the plugin pane `id`. Its keys are text when its
    /// view is a line to type into.
    pub(crate) fn focus_plugin(&mut self, id: PluginViewId) {
        self.plugin_input = self
            .plugin_view(&id)
            .is_some_and(|view| view.content().is_input());
        self.focused_plugin = Some(id);
        self.focus = Focus::Plugin;
    }

    /// Whether the keys go to the focused plugin as text.
    pub(crate) fn plugin_input(&self) -> bool {
        self.focus == Focus::Plugin && self.plugin_input
    }

    /// The editor takes the keys back from the line a plugin shows.
    pub(crate) fn end_plugin_input(&mut self) {
        self.plugin_input = false;
    }

    /// A plugin sent a new view: when it is the pane that has the keyboard and
    /// the view is a line to type into, the keys go to it as text.
    pub(crate) fn plugin_view_changed(&mut self, id: &PluginViewId) {
        if self.focused_plugin.as_ref() != Some(id) {
            return;
        }
        self.plugin_input = self
            .plugin_view(id)
            .is_some_and(|view| view.content().is_input());
    }

    /// Takes a plugin's pane away: its content, and the keyboard if it had it.
    pub(crate) fn remove_plugin_view(&mut self, id: &PluginViewId) {
        self.components.plugin_views.remove(id);
        self.plugin_areas.remove(id);
        if self.focused_plugin.as_ref() == Some(id) {
            self.focused_plugin = None;
            self.plugin_input = false;
            if self.focus == Focus::Plugin {
                self.focus = Focus::Editor;
            }
        }
    }

    pub(crate) fn set_plugin_area(&mut self, id: PluginViewId, area: Rect) {
        self.plugin_areas.insert(id, area);
    }

    pub(crate) fn plugin_area(&self, id: &PluginViewId) -> Option<Rect> {
        self.plugin_areas.get(id).copied()
    }

    /// The plugin pane drawn at the screen cell, if any.
    pub(crate) fn plugin_at(&self, column: u16, row: u16) -> Option<PluginViewId> {
        let at = ratatui::layout::Position::new(column, row);
        self.plugin_areas
            .iter()
            .find(|(id, area)| area.contains(at) && self.is_showing_plugin(id))
            .map(|(id, _)| id.clone())
    }

    fn is_showing_plugin(&self, id: &PluginViewId) -> bool {
        self.is_showing(&crate::ui::ComponentKind::Plugin(id.clone()))
    }
}
