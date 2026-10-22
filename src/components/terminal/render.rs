use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use super::TerminalPane;
use crate::app::state::{AppState, Focus};
use crate::ui::cursor::dim_cell;
use crate::ui::layout::Placement;

/// Draws the frame, the tab bar when there is more than one tab, and the
/// active shell's screen.
pub(super) fn draw(
    frame: &mut Frame,
    state: &AppState,
    placement: &Placement,
    pane: &TerminalPane,
) {
    let theme = state.theme();
    let title = placement.frame.title_text(placement.kind.title());
    let focused = state.focus() == Focus::Terminal;
    let block = placement.frame.block(theme, title, focused);
    frame.render_widget(block, placement.area);

    if pane.tab_bar.height > 0 {
        frame.render_widget(
            Paragraph::new("").style(theme.style("tab.bar")),
            pane.tab_bar,
        );
        for (index, (label, rect)) in pane.labels().iter().zip(&pane.tab_rects).enumerate() {
            if rect.width == 0 {
                continue;
            }
            let slot = if index == pane.active {
                "tab.active"
            } else {
                "tab.inactive"
            };
            let text = truncate(label, usize::from(rect.width));
            frame.render_widget(Paragraph::new(text).style(theme.style(slot)), *rect);
        }
    }

    if pane.add_button.width > 0 {
        frame.render_widget(
            Paragraph::new(" + ").style(theme.style("tab.bar").patch(theme.style("tab.add"))),
            pane.add_button,
        );
    }

    let inner = pane.text_area;
    if let Some(session) = pane.session() {
        let cursor = session.with_screen(|screen| {
            draw_screen(frame.buffer_mut(), inner, screen);
            draw_scroll_indicator(frame.buffer_mut(), inner, screen.scrollback());
            if let Some(range) = pane.selection_range() {
                draw_selection(
                    frame.buffer_mut(),
                    inner,
                    range,
                    theme.style("editor.selection"),
                );
            }
            // The cursor belongs to the present; it is not where the history is.
            (!screen.hide_cursor() && screen.scrollback() == 0).then(|| screen.cursor_position())
        });

        if let Some((row, column)) = cursor
            && row < inner.height
            && column < inner.width
        {
            let cell = Position::new(inner.x + column, inner.y + row);
            match state.has_keyboard(Focus::Terminal) {
                true => frame.set_cursor_position(cell),
                false => dim_cell(frame, theme, cell),
            }
        }
    }
}

/// Where each tab's label goes in the bar, one after the other from the left.
/// Labels that do not fit get an empty rectangle (or are cut at the edge).
pub(super) fn tab_rects(labels: &[String], bar: Rect) -> Vec<Rect> {
    let end = bar.right();
    let mut x = bar.x;
    labels
        .iter()
        .map(|label| {
            let width = u16::try_from(label.width()).unwrap_or(u16::MAX);
            let width = width.min(end.saturating_sub(x));
            let rect = Rect { x, width, ..bar };
            x += width;
            rect
        })
        .collect()
}

/// `text` cut to at most `width` display columns.
fn truncate(text: &str, width: usize) -> String {
    let mut used = 0;
    let mut out = String::new();
    for c in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w > width {
            break;
        }
        used += w;
        out.push(c);
    }
    out
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
    // A default color is not copied: the cell keeps what the screen was painted
    // with, so a default background shows the theme background, not the terminal's.
    let mut style = Style {
        fg: color_of(cell.fgcolor()),
        bg: color_of(cell.bgcolor()),
        ..Style::default()
    };
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

fn color_of(color: vt100::Color) -> Option<Color> {
    match color {
        vt100::Color::Default => None,
        vt100::Color::Idx(index) => Some(Color::Indexed(index)),
        vt100::Color::Rgb(r, g, b) => Some(Color::Rgb(r, g, b)),
    }
}

/// Says in the bottom right corner how far back the view is, if it is.
pub(super) fn draw_scroll_indicator(buffer: &mut Buffer, area: Rect, offset: usize) {
    if offset == 0 || area.height == 0 {
        return;
    }

    let label = format!(" ↑{offset} ");
    let width = label.chars().count() as u16;
    if width > area.width {
        return;
    }

    let y = area.bottom() - 1;
    let start = area.right() - width;
    for (index, c) in label.chars().enumerate() {
        if let Some(cell) = buffer.cell_mut((start + index as u16, y)) {
            cell.set_char(c);
            cell.set_style(Style::default().add_modifier(Modifier::REVERSED));
        }
    }
}

/// Gives the cells from `first` to `last` (row, column; both included, reading order) the
/// style of a selection.
pub(super) fn draw_selection(
    buffer: &mut Buffer,
    area: Rect,
    (first, last): ((u16, u16), (u16, u16)),
    style: Style,
) {
    for row in first.0..=last.0.min(area.height.saturating_sub(1)) {
        let from = if row == first.0 { first.1 } else { 0 };
        let to = if row == last.0 {
            last.1
        } else {
            area.width.saturating_sub(1)
        };
        for column in from..=to.min(area.width.saturating_sub(1)) {
            if let Some(cell) = buffer.cell_mut((area.x + column, area.y + row)) {
                cell.set_style(style);
            }
        }
    }
}
