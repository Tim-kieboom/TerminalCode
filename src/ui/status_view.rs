use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::state::AppState;

pub(super) fn render(frame: &mut Frame, state: &AppState, area: Rect) {
    let editor = state.editor();
    let head = editor.selections().primary().head();

    let mut text = format!(
        " {}  Ln {}, Col {}",
        editor.display_name(),
        head.line + 1,
        head.column + 1
    );
    if let Some(message) = state.status() {
        text.push_str("  |  ");
        text.push_str(message);
    }

    let style = state.theme().style("status.bar");
    frame.render_widget(Paragraph::new(Line::from(text)).style(style), area);
}
