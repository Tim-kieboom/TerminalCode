use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, List, ListState, Paragraph};

use crate::component::ComponentKind;
use crate::state::AppState;
use crate::ui::layout::Placement;
use crate::ui::view::ViewNode;

mod editor_view;
pub mod layout;
mod status_view;
#[cfg(test)]
mod tests;
mod text_layout;
pub mod theme;
pub mod view;

pub(crate) fn render(frame: &mut Frame, state: &mut AppState) {
    for placement in state.layout().resolve(frame.area()) {
        match placement.kind {
            ComponentKind::Editor => editor_view::render(frame, state, placement.area),
            ComponentKind::StatusBar => status_view::render(frame, state, placement.area),
            _ => render_component(frame, state, &placement),
        }
    }
}

fn render_component(frame: &mut Frame, state: &AppState, placement: &Placement) {
    let theme = state.theme();
    let block = Block::bordered()
        .title(placement.kind.title())
        .title_style(theme.style("pane.title"))
        .border_style(theme.style("pane.border"));

    let inner = block.inner(placement.area);
    frame.render_widget(block, placement.area);

    let ComponentKind::Plugin(id) = &placement.kind else {
        return;
    };

    let Some(view) = state.plugin_view(id) else {
        return;
    };

    render_view(frame, state, view.content(), inner);
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
