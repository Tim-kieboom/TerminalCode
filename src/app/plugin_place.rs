//! Where a plugin's view goes: the bar that holds the Explorer, the bar that holds
//! the Terminal, or a popup. What the user chose is kept in `state.json` and put
//! on the layout in memory (see [`crate::app::state::AppState::set_plugin_places`]);
//! the layout file is never written. A plugin the layout file names itself stays
//! where the file puts it.

use ratatui::layout::Position;

use super::App;
use crate::app::state::Focus;
use crate::persist::saved_state::Place;
use crate::ui::{ComponentKind, PluginViewId};

impl App {
    /// The places the layout has room for: a bar that holds the Explorer, a bar
    /// that holds the Terminal, and a popup, which every layout has.
    pub(crate) fn available_places(&self) -> Vec<Place> {
        let layout = self.state.base_layout();
        let mut places = Vec::new();
        if layout.bar_views(&ComponentKind::Explorer).is_some() {
            places.push(Place::Explorer);
        }
        if layout.bar_views(&ComponentKind::Terminal).is_some() {
            places.push(Place::Bottom);
        }
        places.push(Place::Popup);
        places
    }

    /// Whether the layout file puts the plugin's view somewhere itself.
    pub(super) fn layout_places(&self, id: &str) -> bool {
        self.state
            .base_layout()
            .contains(&ComponentKind::Plugin(PluginViewId::new(id)))
    }

    /// Tells the layout where the plugins that run are placed: only those that are
    /// installed, on and placed take a tab, so none is left empty.
    pub(super) fn apply_plugin_places(&mut self) {
        let places = self
            .saved_state()
            .plugins()
            .filter(|(id, record)| {
                record.enabled && self.plugins.manager.is_running(id) && !self.layout_places(id)
            })
            .filter_map(|(id, record)| Some((PluginViewId::new(id), record.place?)))
            .collect();
        self.state.set_plugin_places(places);
    }

    /// Puts the plugin's view in `place` and remembers it. Says why when it cannot.
    pub(crate) fn place_plugin(&mut self, id: &str, place: Place) {
        if self.layout_places(id) {
            self.state
                .notify(format!("plugin {id} is placed by your layout file"));
            return;
        }
        if !self.available_places().contains(&place) {
            self.state.notify(format!(
                "this layout has no {} for plugin {id}",
                place.label()
            ));
            return;
        }
        let mut record = self.saved_plugin_record(id).unwrap_or_default();
        record.place = Some(place);
        self.remember_plugin(id, record);
        self.apply_plugin_places();
        self.state
            .notify(format!("plugin {id} is in the {}", place.label()));
        self.show_placed_plugin(id, place);
    }

    /// Brings the plugin's view into sight where it was placed.
    fn show_placed_plugin(&mut self, id: &str, place: Place) {
        match place {
            Place::Popup => self.open_plugin_popup(id),
            Place::Explorer | Place::Bottom => {
                self.show_bar_view(&ComponentKind::Plugin(PluginViewId::new(id)));
            }
            Place::Unknown => {}
        }
    }

    /// Opens the popup of a plugin placed in one; a plugin placed in a bar gets the
    /// keyboard there.
    pub(crate) fn open_plugin_popup(&mut self, id: &str) {
        if !self.plugins.manager.is_running(id) {
            self.state.notify(format!("plugin {id} is not running"));
            return;
        }
        let place = self.saved_plugin_record(id).and_then(|record| record.place);
        if place != Some(Place::Popup) && !self.layout_places(id) {
            match place {
                Some(_) => self.focus_plugin_view(id),
                None => self.state.notify(format!(
                    "plugin {id} is not placed yet: Plugins: Manage, then Place"
                )),
            }
            return;
        }
        // Opening a plugin is where the user was going: the list does not come back.
        self.plugins.manager.return_to = None;
        let view = PluginViewId::new(id);
        self.state.open_plugin_popup(view.clone());
        self.state.focus_plugin(view);
        self.apply_plugin_keys();
    }

    /// Closes the popup of the plugin and gives the keyboard back to the editor.
    pub(super) fn close_plugin_popup(&mut self) {
        self.state.close_plugin_popup();
        self.state.set_focus(Focus::Editor);
        self.apply_plugin_keys();
        self.needs_redraw = true;
    }

    /// A click outside the open popup of a plugin closes it. Returns whether it did.
    pub(super) fn click_outside_plugin_popup(&mut self, column: u16, row: u16) -> bool {
        let Some(id) = self.state.plugin_popup().cloned() else {
            return false;
        };
        let inside = self
            .state
            .plugin_area(&id)
            .is_some_and(|area| area.contains(Position::new(column, row)));
        if inside {
            return false;
        }
        self.close_plugin_popup();
        true
    }
}
