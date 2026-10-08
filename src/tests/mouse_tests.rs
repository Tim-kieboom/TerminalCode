use crate::event::mouse::{ClickTracker, Clicks};
use std::time::{Duration, Instant};

fn ms(base: Instant, millis: u64) -> Instant {
    base + Duration::from_millis(millis)
}

#[test]
fn a_lone_click_is_single() {
    let mut tracker = ClickTracker::default();

    assert_eq!(tracker.register(Instant::now(), 5, 5), Clicks::Single);
}

#[test]
fn quick_clicks_on_one_cell_count_up_to_triple_then_start_over() {
    let (mut tracker, t0) = (ClickTracker::default(), Instant::now());

    let series: Vec<_> = (0..5)
        .map(|n| tracker.register(ms(t0, n * 100), 5, 5))
        .collect();

    assert_eq!(
        series,
        [
            Clicks::Single,
            Clicks::Double,
            Clicks::Triple,
            Clicks::Single,
            Clicks::Double
        ]
    );
}

#[test]
fn a_slow_second_click_is_a_new_series() {
    let (mut tracker, t0) = (ClickTracker::default(), Instant::now());
    tracker.register(t0, 5, 5);

    assert_eq!(tracker.register(ms(t0, 900), 5, 5), Clicks::Single);
}

#[test]
fn a_click_on_another_cell_is_a_new_series() {
    let (mut tracker, t0) = (ClickTracker::default(), Instant::now());
    tracker.register(t0, 5, 5);

    assert_eq!(tracker.register(ms(t0, 100), 6, 5), Clicks::Single);
    assert_eq!(tracker.register(ms(t0, 200), 6, 6), Clicks::Single);
}

#[test]
fn the_window_is_measured_from_the_previous_click_not_the_first() {
    let (mut tracker, t0) = (ClickTracker::default(), Instant::now());
    tracker.register(t0, 1, 1);
    tracker.register(ms(t0, 300), 1, 1);

    assert_eq!(tracker.register(ms(t0, 600), 1, 1), Clicks::Triple);
}
