//! The parts of the screen: which are shown, and access to each.

use super::{AppState, PluginView};
use crate::components::errors::{ErrorRow, ErrorsView};
use crate::components::explorer::{Explorer, ExplorerError};
use crate::components::menu_bar::MenuBar;
use crate::components::status_view::StatusBar;
use crate::components::terminal::TerminalPane;
use crate::persist::saved_state::Place;
use crate::ui::Hideable;
use crate::ui::layout::{LayoutTree, Placement, Scene};
use crate::ui::view::ViewNode;
use crate::ui::{ComponentKind, PluginViewId};

/// How much of the editor's row a pane right of the editor takes.
const SIDE_PANE_PERCENT: u16 = 40;

impl AppState {
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
            ComponentKind::Errors => self.components.errors.is_shown(),
            ComponentKind::MenuBar => self.components.menu_bar.is_shown(),
            ComponentKind::Editor => true,
        }
    }

    /// Where the components go on a screen of `area`: the ones that are shown,
    /// and for a bar the view it shows.
    pub(crate) fn placements(&self, area: ratatui::layout::Rect) -> Vec<Placement> {
        let mut placements = self.layout.resolve_scene(
            area,
            &Scene {
                visible: &|kind| self.is_visible(kind),
                active: &|first| self.bar_views.get(first).copied().unwrap_or(0),
            },
        );
        self.count_problems_on_tabs(&mut placements);
        placements
    }

    /// The Errors tab of a bar says how many problems there are.
    pub(super) fn count_problems_on_tabs(&self, placements: &mut [Placement]) {
        let count: usize = self
            .lsp_diagnostics
            .documents()
            .map(|(_, diagnostics)| diagnostics.len())
            .sum();
        if count == 0 {
            return;
        }
        for bar in placements
            .iter_mut()
            .filter_map(|placement| placement.bar.as_mut())
        {
            if let Some(at) = bar
                .views
                .iter()
                .position(|view| *view == ComponentKind::Errors)
            {
                bar.labels[at] = format!("Errors ({count})");
            }
        }
    }

    /// The first view of the bar that holds `kind` and the place of `kind` in it.
    pub(super) fn bar_slot(&self, kind: &ComponentKind) -> Option<(ComponentKind, usize)> {
        let views = self.layout.bar_views(kind)?;
        let at = views.iter().position(|view| view == kind)?;
        Some((views[0].clone(), at))
    }

    /// Whether the component is on screen: shown, and for a view of a bar the one
    /// the bar shows.
    pub(crate) fn is_showing(&self, kind: &ComponentKind) -> bool {
        self.is_visible(kind) && self.is_active_view(kind)
    }

    /// Whether `kind` is the view its bar shows; true for a component not in a bar.
    pub(crate) fn is_active_view(&self, kind: &ComponentKind) -> bool {
        match self.bar_slot(kind) {
            Some((first, at)) => self.bar_views.get(&first).copied().unwrap_or(0) == at,
            None => true,
        }
    }

    /// Makes the bar that holds `kind` show it. Does not show a hidden bar.
    pub(crate) fn select_view(&mut self, kind: &ComponentKind) {
        if let Some((first, at)) = self.bar_slot(kind) {
            self.bar_views.insert(first, at);
        }
    }

    /// Shows or hides the component of this kind, if it can be hidden. Showing a
    /// view of a bar makes the bar show it.
    pub(crate) fn set_shown(&mut self, kind: &ComponentKind, shown: bool) {
        if shown {
            self.select_view(kind);
        }
        match kind {
            ComponentKind::Explorer => self.components.explorer.set_shown(shown),
            ComponentKind::Terminal => self.components.terminal.set_shown(shown),
            ComponentKind::StatusBar => self.components.status_bar.set_shown(shown),
            ComponentKind::Errors => self.components.errors.set_shown(shown),
            ComponentKind::MenuBar => self.components.menu_bar.set_shown(shown),
            ComponentKind::Editor | ComponentKind::Plugin(_) => {}
        }
    }

    pub(crate) fn show_explorer(&mut self) {
        self.select_view(&ComponentKind::Explorer);
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

    /// Rows and columns the terminal pane has, or would have if it is shown
    /// now, going by the screen as of the last draw. `None` before the first.
    pub(crate) fn terminal_size_now(&self) -> Option<(u16, u16)> {
        let placements = self.placements(self.screen);
        let placement = placements
            .iter()
            .find(|placement| placement.kind == ComponentKind::Terminal)?;
        let inner = placement.frame.inner(placement.area);
        Some((inner.height, inner.width))
    }

    pub(crate) fn errors(&self) -> &ErrorsView {
        self.components.errors.node()
    }

    pub(crate) fn errors_mut(&mut self) -> &mut ErrorsView {
        self.components.errors.node_mut()
    }

    pub(crate) fn show_errors(&mut self) {
        self.select_view(&ComponentKind::Errors);
        self.components.errors.show();
    }

    pub(crate) fn hide_errors(&mut self) {
        self.components.errors.hide();
    }

    /// The problems the language servers report for the open documents, errors
    /// first, then by file and place.
    pub(crate) fn error_rows(&self) -> Vec<ErrorRow> {
        let project = self.explorer().root_path();
        let mut rows = Vec::new();
        for (document, diagnostics) in self.lsp_diagnostics.documents() {
            let Some((path, name, buffer)) = self.workspace().document_info(document) else {
                continue;
            };
            for diagnostic in diagnostics {
                let Ok(position) = buffer.byte_to_position(diagnostic.range.start) else {
                    continue;
                };
                rows.push(ErrorRow::new(
                    document,
                    path,
                    &name,
                    project,
                    position,
                    diagnostic.severity,
                    &diagnostic.message,
                ));
            }
        }
        crate::components::errors::sort(&mut rows);
        rows
    }

    pub(crate) fn show_terminal(&mut self) {
        self.select_view(&ComponentKind::Terminal);
        self.components.terminal.show();
    }

    pub(crate) fn hide_terminal(&mut self) {
        self.components.terminal.hide();
    }

    pub(crate) fn toggle_menu_bar(&mut self) {
        self.components.menu_bar.toggle();
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

    pub(crate) fn menu_bar(&self) -> &MenuBar {
        self.components.menu_bar.node()
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
        let hide_ignored = self.components.explorer.node().hides_ignored();
        let mut explorer = Explorer::open(root)?;
        explorer.set_hide_ignored(hide_ignored);
        *self.components.explorer.node_mut() = explorer;
        Ok(())
    }

    /// Takes the layout the file describes; the views of plugins placed in a bar
    /// are added to it again.
    pub(crate) fn set_layout(&mut self, layout: LayoutTree) {
        self.base_layout = layout;
        self.rebuild_layout();
    }

    /// The layout as the file says it, without the views the user placed.
    pub(crate) fn base_layout(&self) -> &LayoutTree {
        &self.base_layout
    }

    /// Sets where the plugins that run are placed; a plugin the layout file names
    /// itself stays where the file puts it.
    pub(crate) fn set_plugin_places(&mut self, places: Vec<(PluginViewId, Place)>) {
        if self.plugin_places == places {
            return;
        }
        self.plugin_places = places;
        self.rebuild_layout();
    }

    fn rebuild_layout(&mut self) {
        let mut layout = self.base_layout.clone();
        for (id, place) in &self.plugin_places {
            let anchor = match place {
                Place::Explorer => ComponentKind::Explorer,
                Place::Bottom => ComponentKind::Terminal,
                Place::Side => {
                    let view = ComponentKind::Plugin(id.clone());
                    if let Some(with) = layout.with_view_beside_editor(&view, SIDE_PANE_PERCENT) {
                        layout = with;
                    }
                    continue;
                }
                Place::Popup | Place::Unknown => continue,
            };
            let view = ComponentKind::Plugin(id.clone());
            if let Some(with) = layout.with_view_in_bar_of(&anchor, &view) {
                layout = with;
            }
        }
        self.layout = layout;
    }

    pub(crate) fn layout(&self) -> &LayoutTree {
        &self.layout
    }

    /// The ids of the plugin views that exist, in name order.
    pub(crate) fn plugin_view_ids(&self) -> Vec<PluginViewId> {
        let mut ids: Vec<_> = self.components.plugin_views.keys().cloned().collect();
        ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        ids
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
