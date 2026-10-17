//! The tab strip of a bar: one label per view in the title row of the frame,
//! the shown view highlighted.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use super::layout::Placement;
use super::pane_frame::TitleAlign;
use super::theme::Theme;

/// Between two labels.
const SEPARATOR: &str = " - ";

/// Where each tab of the bar `placement` belongs to is drawn, with the index of
/// its view. Empty for a placement that is not a bar with several views. Labels
/// that do not fit are cut at the edge or left out.
pub fn tab_rects(placement: &Placement) -> Vec<(Rect, usize)> {
    let Some(bar) = &placement.bar else {
        return Vec::new();
    };
    let (row, from, to) = placement.frame.tab_row(placement.area);
    let widths: Vec<u16> = bar
        .labels
        .iter()
        .map(|label| u16::try_from(label.width()).unwrap_or(u16::MAX))
        .collect();
    let separator = SEPARATOR.width() as u16;
    let total: u16 = widths.iter().sum::<u16>() + separator * (widths.len() as u16 - 1);
    // One cell of padding before the first label.
    let room = to.saturating_sub(from);
    let start = match placement.frame.tab_align() {
        TitleAlign::Left => from + 1,
        TitleAlign::Center => from + room.saturating_sub(total) / 2,
        TitleAlign::Right => to.saturating_sub(total + 1),
    }
    .max(from);

    let mut x = start;
    let mut rects = Vec::with_capacity(widths.len());
    for (index, width) in widths.iter().enumerate() {
        let width = (*width).min(to.saturating_sub(x));
        if width > 0 {
            rects.push((
                Rect {
                    x,
                    y: row,
                    width,
                    height: 1,
                },
                index,
            ));
        }
        x = x.saturating_add(width + separator);
    }
    rects
}

/// Draws the strip over the title row of a bar with several views. The frame
/// reserved the row and left it empty.
pub fn draw_tabs(frame: &mut Frame, theme: &Theme, placement: &Placement) {
    let Some(bar) = &placement.bar else {
        return;
    };
    let separator_style = theme.style("pane.title");
    for (rect, index) in tab_rects(placement) {
        let slot = if index == bar.active {
            "bar.tab.active"
        } else {
            "bar.tab.inactive"
        };
        let text = fit(&bar.labels[index], usize::from(rect.width));
        frame.render_widget(Paragraph::new(text).style(theme.style(slot)), rect);
        if index + 1 < bar.views.len() {
            let after = rect.right();
            let room = placement.area.right().saturating_sub(after);
            if room > 0 {
                let line = Line::from(Span::styled(SEPARATOR, separator_style));
                let at = Rect {
                    x: after,
                    y: rect.y,
                    width: (SEPARATOR.width() as u16).min(room),
                    height: 1,
                };
                frame.render_widget(Paragraph::new(line), at);
            }
        }
    }
}

/// `text` cut to at most `width` display columns.
fn fit(text: &str, width: usize) -> String {
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
