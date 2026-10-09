use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, List, ListState, Paragraph},
};

use crate::{
    app::state::{AppState, PluginView, Popup},
    components::ComponentKind,
    ui::{Render, layout::Placement, view::ViewNode},
};

/// Draws every component of the layout: first all `prepare` steps, then all
/// read-only draws.
pub fn prepare_and_render(frame: &mut Frame, state: &mut AppState) {
    paint_background(frame, state);
    let placements = state
        .layout()
        .resolve_visible(frame.area(), &|kind| state.is_visible(kind));

    for placement in &placements {
        prepare_component(state, placement);
    }

    for placement in &placements {
        draw_component(frame, state, placement);
    }

    let bottom = placements
        .iter()
        .find(|placement| placement.kind == ComponentKind::StatusBar)
        .map_or(frame.area().bottom(), |placement| placement.area.y);

    state.notifications().render(frame, state.theme(), bottom);

    let screen = frame.area();
    if let Some(menu) = state.menu_mut() {
        menu.place(screen);
    }

    let theme = state.theme();
    match state.popup() {
        Popup::None => {}
        Popup::Search(search) => search.render(frame, theme),
        Popup::Finder(finder) => finder.render(frame, theme),
        Popup::Palette(palette) => palette.render(frame, theme),
        Popup::QuitPrompt(prompt) => prompt.render(frame, theme),
        Popup::Confirm(confirm) => confirm.render(frame, theme),
        Popup::NewEntry(prompt) => prompt.render(frame, theme),
        Popup::Menu(menu) => menu.render(frame, theme),
        // The find bar is drawn with its pane, which knows where its row is.
        Popup::Find(_) => {}
    }
}

/// Fills the whole screen with the theme's background, if it has one, so that
/// everything drawn afterwards sits on it. Without one the terminal's own
/// background (and its opacity or blur) shows through.
fn paint_background(frame: &mut Frame, state: &AppState) {
    let Some(color) = state.theme().background_color() else {
        return;
    };
    frame.render_widget(Block::new().style(Style::new().bg(color)), frame.area());
}

fn prepare_component(state: &mut AppState, placement: &Placement) {
    match placement.kind {
        ComponentKind::Editor => {
            state.workspace_mut().prepare(placement);
            let (workspace, theme) = state.workspace_and_theme_mut();
            for error in workspace.refresh_highlights(theme) {
                state.notify_error(error);
            }
        }
        ComponentKind::Explorer => state.explorer_mut().prepare(placement),
        ComponentKind::StatusBar => state.status_bar_mut().prepare(placement),
        _ => (),
    }
}

fn draw_component(frame: &mut Frame, state: &AppState, placement: &Placement) {
    match placement.kind {
        ComponentKind::Editor => state.workspace().render(frame, state, placement),
        ComponentKind::Explorer => state.explorer().render(frame, state, placement),
        ComponentKind::StatusBar => state.status_bar().render(frame, state, placement),
        _ => render_plain(frame, state, placement),
    }
}

fn render_plain(frame: &mut Frame, state: &AppState, placement: &Placement) {
    let title = placement.frame.title_text(placement.kind.title());
    let block = placement.frame.block(state.theme(), title, false);

    frame.render_widget(block, placement.area);

    let ComponentKind::Plugin(id) = &placement.kind else {
        return;
    };

    let Some(view) = state.plugin_view(&id) else {
        return;
    };

    view.render(frame, state, placement);
}

/// A plugin's content, inside the frame the layout gives its pane.
impl Render for PluginView {
    fn render(&self, frame: &mut Frame, state: &AppState, placement: &Placement) {
        let inner = placement.frame.inner(placement.area);
        render_view(frame, state, self.content(), inner);
    }
}

fn render_view(frame: &mut Frame, state: &AppState, node: &ViewNode, area: Rect) {
    let theme = state.theme();
    match node {
        ViewNode::Lines(lines) => {
            let lines: Vec<Line> = lines
                .iter()
                .map(|line| Line::styled(line.text.as_ref(), theme.style(&line.slot)))
                .collect();

            frame.render_widget(Paragraph::new(lines), area);
        }
        ViewNode::List { items, selected } => {
            let list = List::new(items.iter().map(|item| item.as_ref()))
                .highlight_style(theme.style("list.selected"));

            let mut list_state = ListState::default().with_selected(*selected);
            frame.render_stateful_widget(list, area, &mut list_state);
        }
    }
}
