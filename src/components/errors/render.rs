use ratatui::Frame;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{COPY_BUTTON, ErrorRow, ErrorsView};
use crate::app::state::{AppState, Focus};
use crate::components::editor::render::slot_of;
use crate::ui::layout::Placement;

const EMPTY: &str = "no problems found (a language server reports them)";

impl ErrorsView {
    /// Draws the frame and the rows.
    pub(crate) fn render(
        &self,
        frame: &mut Frame,
        state: &AppState,
        placement: &Placement,
        rows: &[ErrorRow],
    ) {
        let theme = state.theme();
        let focused = state.focus() == Focus::Errors;
        let title = placement.frame.title_text(placement.kind.title());
        let block = placement.frame.block(theme, title, focused);
        let inner = block.inner(placement.area);
        frame.render_widget(block, placement.area);

        // Like the button of a message, but dimmed while no problem is selected.
        if let Some(button) = self.copy_button() {
            let style = theme.style(placement.frame.title_slot());
            let style = match self.selected_row(rows) {
                Some(_) => style.add_modifier(Modifier::BOLD),
                None => style.add_modifier(Modifier::DIM),
            };
            frame.render_widget(Paragraph::new(COPY_BUTTON).style(style), button);
        }

        if rows.is_empty() {
            frame.render_widget(
                Paragraph::new(EMPTY).style(theme.style("errors.empty")),
                inner,
            );
            return;
        }

        let live = state.has_keyboard(Focus::Errors);
        let width = rows
            .iter()
            .map(|row| row.place().chars().count())
            .max()
            .unwrap_or(0);
        let lines: Vec<Line> = rows
            .iter()
            .enumerate()
            .skip(self.scroll())
            .take(usize::from(inner.height))
            .map(|(index, row)| {
                let place = format!("{:<width$}  ", row.place());
                let line = Line::from(vec![
                    Span::styled(place, theme.style("errors.place")),
                    Span::styled(row.message.clone(), theme.style(slot_of(row.severity))),
                ]);
                if !self.is_selected(index) {
                    return line;
                }
                let slot = match live {
                    true => "errors.selected",
                    false => "errors.selected.inactive",
                };
                line.style(theme.style(slot))
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), inner);
    }
}
