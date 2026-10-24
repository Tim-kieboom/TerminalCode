use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, List, ListState, Paragraph},
};

use unicode_width::UnicodeWidthStr;

use crate::{
    app::state::{AppState, Focus, PluginView, Popup},
    ui::{
        ComponentKind, Render, backdrop, bar,
        layout::Placement,
        view::{ViewNode, button_cells, rows_scroll},
    },
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
    if let Some(id) = state.plugin_popup().cloned() {
        let area = plugin_popup_area(screen);
        let inner = Block::bordered().inner(area);
        state.set_plugin_area(id, area, inner);
    }
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
        Popup::Actions(list) | Popup::RunPicker(list) => list.render(frame, theme),
        Popup::RootPassword(prompt) => prompt.render(frame, theme),
        Popup::PluginView(id) => draw_plugin_popup(frame, state, id, screen),
        Popup::PluginList(list) => list.render(frame, theme),
        Popup::PluginReadme(readme) => readme.render(frame, theme),
        Popup::PluginKey(capture) => capture.render(frame, theme),
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
    match &placement.kind {
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
        ComponentKind::Plugin(id) => {
            let inner = placement.frame.inner(placement.area);
            state.set_plugin_area(id.clone(), placement.area, inner);
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

/// Where the popup of a plugin goes: most of the screen, in the middle.
pub(crate) fn plugin_popup_area(screen: Rect) -> Rect {
    let width = (screen.width * 7 / 10)
        .max(40.min(screen.width))
        .min(screen.width);
    let height = (screen.height * 7 / 10)
        .max(10.min(screen.height))
        .min(screen.height);
    Rect {
        x: screen.x + (screen.width - width) / 2,
        y: screen.y + (screen.height - height) / 2,
        width,
        height,
    }
}

/// A plugin's view over the editor, in a frame with the plugin's name.
fn draw_plugin_popup(
    frame: &mut Frame,
    state: &AppState,
    id: &crate::ui::PluginViewId,
    screen: Rect,
) {
    let theme = state.theme();
    let area = plugin_popup_area(screen);
    let block = Block::bordered()
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(theme.style("pane.border.focused"))
        .title(Line::styled(
            format!(" {} ", id.as_str()),
            theme.style("pane.title"),
        ));
    let inner = block.inner(area);
    backdrop::clear(frame, theme, area);
    frame.render_widget(block, area);
    if let Some(view) = state.plugin_view(id) {
        let typing = state.plugin_input() && state.focused_plugin() == Some(id);
        render_view(frame, state, view.content(), inner, typing, true);
    }
}

fn render_plain(frame: &mut Frame, state: &AppState, placement: &Placement) {
    let title = placement.frame.title_text(placement.kind.title());
    let focused = matches!(&placement.kind, ComponentKind::Plugin(id)
        if state.focused_plugin() == Some(id));
    let block = placement.frame.block(state.theme(), title, focused);

    frame.render_widget(block, placement.area);

    let ComponentKind::Plugin(id) = &placement.kind else {
        return;
    };

    let Some(view) = state.plugin_view(id) else {
        return;
    };

    view.render(frame, state, placement);
}

/// A plugin's content, inside the frame the layout gives its pane.
impl Render<AppState> for PluginView {
    fn render(&self, frame: &mut Frame, state: &AppState, placement: &Placement) {
        let inner = placement.frame.inner(placement.area);
        let typing = matches!(&placement.kind, ComponentKind::Plugin(id)
            if state.plugin_input() && state.focused_plugin() == Some(id));
        let has_keys = matches!(&placement.kind, ComponentKind::Plugin(id)
            if state.focused_plugin() == Some(id));
        render_view(frame, state, self.content(), inner, typing, has_keys);
    }
}

/// `typing`: the keys go to the plugin as text, so a line to type into gets the
/// terminal cursor. A column is drawn part by part.
fn render_view(
    frame: &mut Frame,
    state: &AppState,
    node: &ViewNode,
    area: Rect,
    typing: bool,
    has_keys: bool,
) {
    for (part_area, part) in node.leaves(area) {
        render_part(frame, state, part, part_area, typing, has_keys);
    }
}

fn render_part(
    frame: &mut Frame,
    state: &AppState,
    node: &ViewNode,
    area: Rect,
    typing: bool,
    has_keys: bool,
) {
    let theme = state.theme();
    match node {
        // A column is cut into its parts before they get here.
        ViewNode::Column(_) => {}
        ViewNode::Buttons(buttons) => {
            let cells = button_cells(buttons, area.width);
            for (button, (line, start, width)) in buttons.iter().zip(cells) {
                let Some(x) = area.x.checked_add(start).filter(|x| *x < area.right()) else {
                    continue;
                };
                let y = area.y.saturating_add(line);
                if y >= area.bottom() {
                    break;
                }
                let rect = Rect::new(x, y, width.min(area.right() - x), 1);
                let slot = match button.enabled {
                    true => "bar.tab.active",
                    false => "bar.tab.inactive",
                };
                let text = format!(" {} ", button.label);
                frame.render_widget(Paragraph::new(Span::styled(text, theme.style(slot))), rect);
            }
        }
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
        ViewNode::Rows { rows, selected } => {
            let height = usize::from(area.height);
            let top = rows_scroll(*selected, rows.len(), height);
            let focused = typing || has_keys;
            let highlight = match focused {
                true => "explorer.selected",
                false => "explorer.selected.inactive",
            };
            let lines: Vec<Line> = rows
                .iter()
                .enumerate()
                .skip(top)
                .take(height)
                .map(|(index, row)| {
                    let spans: Vec<Span> = row
                        .spans
                        .iter()
                        .map(|span| Span::styled(span.text.as_ref(), theme.style(&span.slot)))
                        .collect();
                    let line = Line::from(spans);
                    match Some(index) == *selected {
                        // A line style covers the whole row, not just the text.
                        true => line.style(theme.style(highlight)),
                        false => line,
                    }
                })
                .collect();
            frame.render_widget(Paragraph::new(lines), area);
        }
        ViewNode::Input {
            prompt,
            value,
            cursor,
            active,
        } => {
            let line = Line::from(vec![
                Span::styled(prompt.as_ref(), theme.style("explorer.hint")),
                Span::raw(value.as_ref()),
            ]);
            frame.render_widget(Paragraph::new(line), area);
            if typing && *active && area.width > 0 && area.height > 0 {
                let before: String = value.chars().take(*cursor).collect();
                let column = prompt.width() + before.width();
                let column = u16::try_from(column).unwrap_or(u16::MAX);
                let column = column.min(area.width - 1);
                frame.set_cursor_position((area.x + column, area.y));
            }
        }
    }
}
