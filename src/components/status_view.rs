use ratatui::Frame;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::app::state::AppState;
use crate::ui::Render;
use crate::ui::layout::Placement;

#[derive(Debug, Default)]
pub struct StatusBar;
impl Render for StatusBar {
    fn render(&self, frame: &mut Frame, state: &AppState, placement: &Placement) {
        let text = if state.terminal_prefix_pending() {
            " ctrl+b ...  e: editor  x: explorer  t: hide  c: new tab  n/p/1-9: tabs  w: close tab  ctrl+b: send ctrl+b  esc: cancel"
                .to_owned()
        } else if let Some(keys) = state.pending_keys() {
            format!(" {keys} ...  waiting for the next key")
        } else if state.workspace().has_tabs() {
            let editor = state.editor();
            let head = editor.selections().primary().head();
            let server = state
                .lsp_status()
                .map(|status| format!("  [{status}]"))
                .unwrap_or_default();
            format!(
                " {}  Ln {}, Col {}{server} -- quit: Ctr+Q, commandPallet: f1",
                editor.display_name(),
                head.line + 1,
                head.column + 1
            )
        } else {
            " no open files -- new file: Ctrl+N, quit: Ctrl+Q".to_owned()
        };

        let theme = state.theme();
        let title = placement.frame.title_text("Status");
        let block = placement.frame.block(theme, title, false);
        let inner = block.inner(placement.area);
        frame.render_widget(block, placement.area);

        let style = theme.style("status.bar");
        frame.render_widget(Paragraph::new(Line::from(text)).style(style), inner);
    }
}
