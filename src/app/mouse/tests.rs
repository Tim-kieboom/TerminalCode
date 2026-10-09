use std::time::{Duration, Instant};

use super::MouseInput;
use crate::event::mouse::Clicks;

#[test]
fn the_mouse_starts_on_with_nothing_to_tell_the_terminal() {
    let mut mouse = MouseInput::default();

    assert!(mouse.is_enabled());
    assert_eq!(mouse.take_change(), None);
}

#[test]
fn toggling_flips_it_and_records_one_change_for_the_terminal() {
    let mut mouse = MouseInput::default();

    assert!(!mouse.toggle());
    assert!(!mouse.is_enabled());
    assert_eq!(mouse.take_change(), Some(false));
    assert_eq!(mouse.take_change(), None, "told once");

    assert!(mouse.toggle());
    assert_eq!(mouse.take_change(), Some(true));
}

#[test]
fn a_change_toggled_back_before_the_terminal_was_told_is_only_the_latest() {
    let mut mouse = MouseInput::default();

    mouse.toggle();
    mouse.toggle();

    assert_eq!(mouse.take_change(), Some(true));
}

#[test]
fn quick_clicks_on_one_cell_count_up() {
    let mut mouse = MouseInput::default();
    let start = Instant::now();

    assert_eq!(mouse.register_click(start, 5, 5), Clicks::Single);
    assert_eq!(
        mouse.register_click(start + Duration::from_millis(100), 5, 5),
        Clicks::Double
    );
    assert_eq!(
        mouse.register_click(start + Duration::from_millis(200), 5, 5),
        Clicks::Triple
    );
}

#[test]
fn a_click_on_another_cell_starts_over() {
    let mut mouse = MouseInput::default();
    let start = Instant::now();
    mouse.register_click(start, 5, 5);

    let next = mouse.register_click(start + Duration::from_millis(100), 9, 5);

    assert_eq!(next, Clicks::Single);
}
