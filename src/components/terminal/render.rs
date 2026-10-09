use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::app::state::{AppState, Focus};
use crate::pty::Session;
use crate::ui::layout::Placement;

/// Draws the frame and, inside it, the shell's screen.
pub(super) fn draw(
    frame: &mut Frame,
    state: &AppState,
    placement: &Placement,
    session: Option<&Session>,
) {
    let title = placement.frame.title_text(placement.kind.title());
    let focused = state.focus() == Focus::Terminal;
    let block = placement.frame.block(state.theme(), title, focused);
    let inner = block.inner(placement.area);

    frame.render_widget(block, placement.area);
    if let Some(session) = session {
        let cursor = session.with_screen(|screen| {
            draw_screen(frame.buffer_mut(), inner, screen);
            (!screen.hide_cursor()).then(|| screen.cursor_position())
        });

        if let (true, Some((row, column))) = (focused, cursor)
            && row < inner.height
            && column < inner.width
        {
            frame.set_cursor_position((inner.x + column, inner.y + row));
        }
    }
}

/// Copies the cells of `screen` into `buffer` inside `area`. A screen larger
/// than the area is cut; a smaller one leaves the rest of the area as it was.
pub(super) fn draw_screen(buffer: &mut Buffer, area: Rect, screen: &vt100::Screen) {
    let (rows, columns) = screen.size();
    for row in 0..rows.min(area.height) {
        let mut column = 0;
        while column < columns.min(area.width) {
            let Some(cell) = screen.cell(row, column) else {
                break;
            };
            let target = buffer.cell_mut((area.x + column, area.y + row));
            let Some(target) = target else {
                break;
            };
            let text = cell.contents();
            target.set_symbol(if text.is_empty() { " " } else { text });
            target.set_style(style_of(cell));
            // The second half of a wide character has no symbol of its own.
            column += if cell.is_wide() { 2 } else { 1 };
        }
    }
}

fn style_of(cell: &vt100::Cell) -> Style {
    let mut style = Style::default()
        .fg(color_of(cell.fgcolor()))
        .bg(color_of(cell.bgcolor()));
    for (on, modifier) in [
        (cell.bold(), Modifier::BOLD),
        (cell.dim(), Modifier::DIM),
        (cell.italic(), Modifier::ITALIC),
        (cell.underline(), Modifier::UNDERLINED),
        (cell.inverse(), Modifier::REVERSED),
    ] {
        if on {
            style = style.add_modifier(modifier);
        }
    }
    style
}

fn color_of(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(index) => Color::Indexed(index),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}
