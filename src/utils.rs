use std::rc::Rc;

use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub fn horizontal_layout<I>(constraints: I, area: Rect) -> Rc<[Rect]> 
where 
    I: IntoIterator,
    I::Item: Into<Constraint>
{
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area)
}

pub fn vertical_layout<I>(constraints: I, area: Rect) -> Rc<[Rect]> 
where 
    I: IntoIterator,
    I::Item: Into<Constraint>
{
    Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area)
}