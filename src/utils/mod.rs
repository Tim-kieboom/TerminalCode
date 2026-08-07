use std::rc::Rc;

use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub fn horizontal_layout<I>(constraints: I, area: Rect) -> Rc<[Rect]>
where
    I: IntoIterator,
    I::Item: Into<Constraint>,
{
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area)
}

pub fn vertical_layout<I>(constraints: I, area: Rect) -> Rc<[Rect]>
where
    I: IntoIterator,
    I::Item: Into<Constraint>,
{
    Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area)
}

pub fn popup_layout(area: Rect, width: u16, height: u16) -> Rect {
    let half_width = (area.width - width) / 2;
    let horizontal = horizontal_layout(
        [
            Constraint::Length(half_width),
            Constraint::Length(width),
            Constraint::Min(0),
        ],
        area,
    );

    let half_height = (area.height - height) / 2;

    let vertical = vertical_layout(
        [
            Constraint::Length(half_height),
            Constraint::Length(height),
            Constraint::Min(0),
        ],
        horizontal[1],
    );

    vertical[1]
}

#[cfg(test)]
mod tests;
