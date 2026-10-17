//! What a box that covers other text starts from.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Clear};

use super::theme::{BackgroundPane, Theme};

/// Wipes `area` and gives it the theme's background, if it paints one, so a box
/// drawn over the text looks like the rest of the screen. A slot of its own
/// that sets a `background` still wins when it is drawn on top. With a
/// transparent background nothing is painted, as for the whole screen. The
/// theme's `[background.popup]` table, if it has one, replaces the screen's.
pub fn clear(frame: &mut Frame, theme: &Theme, area: Rect) {
    frame.render_widget(Clear, area);
    if let Some(color) = theme.pane_color(BackgroundPane::Popup) {
        frame.render_widget(Block::new().style(Style::new().bg(color)), area);
    }
}

/// Gives a pane the background its `[background.<pane>]` table asks for, over
/// the screen's. A pane without a table is left as the screen was painted; a
/// transparent one is wiped, so the terminal's own background shows.
pub fn paint_pane(frame: &mut Frame, theme: &Theme, pane: BackgroundPane, area: Rect) {
    match theme.pane_background(pane) {
        None => {}
        Some(None) => frame.render_widget(Clear, area),
        Some(Some(color)) => {
            frame.render_widget(Clear, area);
            frame.render_widget(Block::new().style(Style::new().bg(color)), area);
        }
    }
}
