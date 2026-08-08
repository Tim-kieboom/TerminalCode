use ratatui::layout::{Constraint, Rect};

use super::{horizontal_layout, popup_layout, vertical_layout};

#[test]
fn horizontal_layout_respects_lengths() {
    let area = Rect::new(0, 0, 100, 50);
    let result = horizontal_layout([Constraint::Length(30), Constraint::Min(70)], area);

    assert_eq!(result.len(), 2);
    assert_eq!(result[0], Rect::new(0, 0, 30, 50));
    assert_eq!(result[1], Rect::new(30, 0, 70, 50));
}

#[test]
fn vertical_layout_respects_lengths() {
    let area = Rect::new(0, 0, 100, 50);
    let result = vertical_layout([Constraint::Length(20), Constraint::Min(30)], area);

    assert_eq!(result.len(), 2);
    assert_eq!(result[0], Rect::new(0, 0, 100, 20));
    assert_eq!(result[1], Rect::new(0, 20, 100, 30));
}

#[test]
fn popup_layout_centers_popup() {
    let area = Rect::new(0, 0, 100, 60);
    let popup = popup_layout(area, 40, 20);
    assert_eq!(popup, Rect::new(30, 20, 40, 20));
}

#[test]
fn popup_layout_centers_within_offset_area() {
    let area = Rect::new(10, 5, 30, 20);
    let popup = popup_layout(area, 20, 10);
    assert_eq!(popup, Rect::new(15, 10, 20, 10));
}

#[test]
fn popup_layout_fills_area_when_equal_size() {
    let area = Rect::new(0, 0, 20, 10);
    let popup = popup_layout(area, 20, 10);
    assert_eq!(popup, Rect::new(0, 0, 20, 10));
}
