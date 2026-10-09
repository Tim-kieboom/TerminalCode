use ratatui::style::{Color, Style};

use crate::syntax::highlighter::{Found, flatten};

fn style(color: Color) -> Style {
    Style::default().fg(color)
}

fn found(start: usize, end: usize, pattern: usize, color: Color) -> Found {
    Found {
        start,
        end,
        pattern,
        style: style(color),
    }
}

fn run(found: Vec<Found>, range: std::ops::Range<usize>) -> Vec<(usize, usize, Color)> {
    flatten(found, &range)
        .into_iter()
        .map(|span| {
            (
                span.range.start,
                span.range.end,
                span.style.fg.unwrap_or(Color::Reset),
            )
        })
        .collect()
}

#[test]
fn separate_captures_stay_separate() {
    let spans = run(
        vec![found(0, 2, 0, Color::Red), found(5, 7, 0, Color::Blue)],
        0..10,
    );

    assert_eq!(spans, [(0, 2, Color::Red), (5, 7, Color::Blue)]);
}

#[test]
fn an_inner_capture_wins_and_the_outer_shows_around_it() {
    let spans = run(
        vec![found(0, 10, 0, Color::Red), found(3, 5, 1, Color::Blue)],
        0..10,
    );

    assert_eq!(
        spans,
        [(0, 3, Color::Red), (3, 5, Color::Blue), (5, 10, Color::Red)]
    );
}

#[test]
fn the_first_pattern_wins_for_the_same_range() {
    let spans = run(
        vec![found(2, 6, 3, Color::Blue), found(2, 6, 1, Color::Red)],
        0..10,
    );

    assert_eq!(spans, [(2, 6, Color::Red)]);
}

#[test]
fn neighbours_with_the_same_style_are_joined() {
    let spans = run(
        vec![found(0, 3, 0, Color::Red), found(3, 6, 0, Color::Red)],
        0..10,
    );

    assert_eq!(spans, [(0, 6, Color::Red)]);
}

#[test]
fn captures_are_cut_at_the_edges_of_the_range() {
    let spans = run(vec![found(0, 20, 0, Color::Red)], 5..9);

    assert_eq!(spans, [(5, 9, Color::Red)]);
}

#[test]
fn an_inner_capture_cannot_stick_out_of_the_one_around_it() {
    let spans = run(
        vec![found(0, 5, 0, Color::Red), found(3, 9, 1, Color::Blue)],
        0..10,
    );

    assert_eq!(spans, [(0, 3, Color::Red), (3, 5, Color::Blue)]);
}

#[test]
fn deep_nesting_unwinds_in_order() {
    let spans = run(
        vec![
            found(0, 12, 0, Color::Red),
            found(2, 10, 1, Color::Green),
            found(4, 6, 2, Color::Blue),
        ],
        0..12,
    );

    assert_eq!(
        spans,
        [
            (0, 2, Color::Red),
            (2, 4, Color::Green),
            (4, 6, Color::Blue),
            (6, 10, Color::Green),
            (10, 12, Color::Red),
        ]
    );
}

#[test]
fn no_captures_give_no_spans() {
    assert!(run(Vec::new(), 0..10).is_empty());
}
