use ratatui::Frame;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::state::AppState;
use crate::ui::Render;
use crate::ui::layout::Placement;

#[derive(Debug, Default)]
pub struct StatusBar;
impl Render for StatusBar {
    fn render(&self, frame: &mut Frame, state: &AppState, placement: &Placement) {
        let mut text = if state.workspace().has_tabs() {
            let editor = state.editor();
            let head = editor.selections().primary().head();
            format!(
                " {}  Ln {}, Col {} -- quit: Ctr+Q, showKeybinds: ?",
                editor.display_name(),
                head.line + 1,
                head.column + 1
            )
        } else {
            " no open files -- new file: Ctrl+N, quit: Ctrl+Q".to_owned()
        };
        if let Some(message) = state.status() {
            text.push_str("  |  ");
            text.push_str(message);
        }

        let style = state.theme().style("status.bar");
        frame.render_widget(
            Paragraph::new(Line::from(text)).style(style),
            placement.area,
        );
    }
}
