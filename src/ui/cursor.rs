//! The marks for a cursor that does not have the keyboard.

use ratatui::Frame;
use ratatui::layout::Position;

use super::theme::Theme;

/// Marks the cell at `cell` as where a cursor is that typing does not go to:
/// the text stays, the cell gets the theme's `cursor.unfocused` look.
pub(crate) fn dim_cell(frame: &mut Frame, theme: &Theme, cell: Position) {
    if let Some(target) = frame.buffer_mut().cell_mut(cell) {
        target.set_style(theme.unfocused_cursor());
    }
}
