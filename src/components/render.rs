use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, List, ListState, Paragraph},
};

use crate::{
    app::state::{AppState, Focus, PluginView, Popup},
    ui::{ComponentKind, Render, backdrop, bar, layout::Placement, view::ViewNode},
};

/// Draws every component of the layout: first all `prepare` steps, then all
/// read-only draws.
pub fn prepare_and_render(frame: &mut Frame, state: &mut AppState) {
    state.set_screen(frame.area());
    paint_background(frame, state);
    let placements = state.placements(frame.area());

    for placement in &placements {
        prepare_component(state, placement);
    }

    for placement in &placements {
        if let Some(pane) = placement.kind.background_pane() {
            backdrop::paint_pane(frame, state.theme(), pane, placement.area);
        }
        draw_component(frame, state, placement);
        bar::draw_tabs(frame, state.theme(), placement);
    }

    let bottom = placements
        .iter()
        .find(|placement| placement.kind == ComponentKind::StatusBar)
        .map_or(frame.area().bottom(), |placement| placement.area.y);

    let hits = state.notifications().render(frame, state.theme(), bottom);
    state.set_notification_hits(hits);

    let screen = frame.area();
    if let Some(menu) = state.menu_mut() {
        menu.place(screen);
    }

    if let Some(prompt) = state.name_prompt_mut() {
        prompt.place(screen);
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
        Popup::Diagnostics(popup) => popup.render(frame, theme),
        Popup::Definitions(list) => list.render(frame, theme),
        Popup::Actions(list) => list.render(frame, theme),
        Popup::RootPassword(prompt) => prompt.render(frame, theme),
        // The find bar is drawn with its pane, which knows where its row is.
        Popup::Find(_) => {}
    }

    // The suggestions sit by the cursor, and only while no popup covers the editor.
    if matches!(state.popup(), Popup::None)
        && state.focus() == Focus::Editor
        && let Some(suggestions) = state.completion_box()
        && let Some(cursor) = state.editor().cursor_cell()
    {
        suggestions.render(frame, state.theme(), cursor);
    }

    // What the server says about the thing under the pointer, over everything but a popup.
    if matches!(state.popup(), Popup::None)
        && let Some(hover) = state.hover_box()
    {
        hover.render(frame, state.theme());
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
            state.workspace_mut().refresh_git_lines();
            for error in state.refresh_highlights() {
                state.notify_error(error);
            }
        }
        ComponentKind::Explorer => state.explorer_mut().prepare(placement),
        ComponentKind::StatusBar => state.status_bar_mut().prepare(placement),
        ComponentKind::Terminal => state.terminal_mut().prepare(placement),
        ComponentKind::Errors => {
            let count = state.error_rows().len();
            let body = placement.frame.inner(placement.area);
            state.errors_mut().prepare(placement.area, body, count);
        }
        _ => (),
    }
}

fn draw_component(frame: &mut Frame, state: &AppState, placement: &Placement) {
    match placement.kind {
        ComponentKind::Editor => state.workspace().render(frame, state, placement),
        ComponentKind::Explorer => state.explorer().render(frame, state, placement),
        ComponentKind::StatusBar => state.status_bar().render(frame, state, placement),
        ComponentKind::MenuBar => state.menu_bar().render(frame, state, placement),
        ComponentKind::Terminal => state.terminal().render(frame, state, placement),
        ComponentKind::Errors => {
            let rows = state.error_rows();
            state.errors().render(frame, state, placement, &rows);
        }
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
impl Render<AppState> for PluginView {
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
