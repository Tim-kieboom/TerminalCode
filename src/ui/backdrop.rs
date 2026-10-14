//! What a box that covers other text starts from.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Clear};

use super::theme::Theme;

/// Wipes `area` and gives it the theme's background, if it paints one, so a box
/// drawn over the text looks like the rest of the screen. A slot of its own
/// that sets a `background` still wins when it is drawn on top. With a
/// transparent background nothing is painted, as for the whole screen.
pub(crate) fn clear(frame: &mut Frame, theme: &Theme, area: Rect) {
    frame.render_widget(Clear, area);
    if let Some(color) = theme.background_color() {
        frame.render_widget(Block::new().style(Style::new().bg(color)), area);
    }
}
