use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};

use crate::components::terminal::render::draw_screen;

/// What the cells of `text`, as a program would print them, look like once
/// drawn in an area of the given size.
fn drawn(text: &str, rows: u16, columns: u16, area: Rect) -> Buffer {
    let mut parser = vt100::Parser::new(rows, columns, 0);
    parser.process(text.as_bytes());
    let mut buffer = Buffer::empty(Rect::new(0, 0, area.right() + 2, area.bottom() + 2));
    draw_screen(&mut buffer, area, parser.screen());
    buffer
}

fn at(buffer: &Buffer, x: u16, y: u16) -> &ratatui::buffer::Cell {
    buffer.cell((x, y)).unwrap()
}

#[test]
fn text_lands_where_the_screen_has_it() {
    let buffer = drawn("ab\r\ncd", 3, 5, Rect::new(1, 1, 5, 3));

    assert_eq!(at(&buffer, 1, 1).symbol(), "a");
    assert_eq!(at(&buffer, 2, 1).symbol(), "b");
    assert_eq!(at(&buffer, 1, 2).symbol(), "c");
    assert_eq!(at(&buffer, 2, 2).symbol(), "d");
}

#[test]
fn empty_cells_are_blank_so_nothing_underneath_shows_through() {
    let mut buffer = Buffer::empty(Rect::new(0, 0, 10, 4));
    buffer.set_string(0, 0, "XXXXXXXXXX", ratatui::style::Style::default());
    let parser = vt100::Parser::new(2, 4, 0);

    draw_screen(&mut buffer, Rect::new(0, 0, 4, 2), parser.screen());

    assert_eq!(at(&buffer, 0, 0).symbol(), " ");
    assert_eq!(at(&buffer, 3, 0).symbol(), " ");
    assert_eq!(
        at(&buffer, 4, 0).symbol(),
        "X",
        "outside the screen is left alone"
    );
}

#[test]
fn colors_become_terminal_colors() {
    let buffer = drawn(
        "\x1b[31mr\x1b[38;5;200mi\x1b[38;2;1;2;3mt\x1b[0mp\x1b[44mb",
        1,
        10,
        Rect::new(0, 0, 10, 1),
    );

    assert_eq!(at(&buffer, 0, 0).fg, Color::Indexed(1));
    assert_eq!(at(&buffer, 1, 0).fg, Color::Indexed(200));
    assert_eq!(at(&buffer, 2, 0).fg, Color::Rgb(1, 2, 3));
    assert_eq!(at(&buffer, 3, 0).fg, Color::Reset);
    assert_eq!(at(&buffer, 4, 0).bg, Color::Indexed(4));
}

#[test]
fn attributes_become_modifiers() {
    let buffer = drawn(
        "\x1b[1mb\x1b[0m\x1b[3mi\x1b[0m\x1b[4mu\x1b[0m\x1b[7mr\x1b[0mp",
        1,
        10,
        Rect::new(0, 0, 10, 1),
    );

    assert!(at(&buffer, 0, 0).modifier.contains(Modifier::BOLD));
    assert!(at(&buffer, 1, 0).modifier.contains(Modifier::ITALIC));
    assert!(at(&buffer, 2, 0).modifier.contains(Modifier::UNDERLINED));
    assert!(at(&buffer, 3, 0).modifier.contains(Modifier::REVERSED));
    assert_eq!(at(&buffer, 4, 0).modifier, Modifier::empty());
}

#[test]
fn a_wide_character_takes_two_cells_and_the_next_one_stays_clear() {
    let buffer = drawn("漢a", 1, 6, Rect::new(0, 0, 6, 1));

    assert_eq!(at(&buffer, 0, 0).symbol(), "漢");
    assert_eq!(at(&buffer, 2, 0).symbol(), "a");
}

#[test]
fn a_screen_larger_than_the_area_is_cut() {
    let buffer = drawn("abcdef\r\nghijkl", 2, 6, Rect::new(0, 0, 3, 1));

    assert_eq!(at(&buffer, 2, 0).symbol(), "c");
    assert_eq!(at(&buffer, 3, 0).symbol(), " ", "not drawn");
    assert_eq!(at(&buffer, 0, 1).symbol(), " ", "the second row is cut");
}

#[test]
fn combining_marks_stay_with_their_letter() {
    let buffer = drawn("e\u{301}x", 1, 4, Rect::new(0, 0, 4, 1));

    assert_eq!(at(&buffer, 0, 0).symbol(), "e\u{301}");
    assert_eq!(at(&buffer, 1, 0).symbol(), "x");
}

#[test]
fn the_scroll_indicator_sits_in_the_bottom_right_corner() {
    use crate::components::terminal::render::draw_scroll_indicator;
    let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 5));

    draw_scroll_indicator(&mut buffer, Rect::new(2, 1, 12, 3), 42);

    // " ↑42 " is five cells wide and ends at the right edge of the area.
    let row: String = (9..14)
        .map(|x| at(&buffer, x, 3).symbol().to_owned())
        .collect();
    assert_eq!(row, " ↑42 ");
    assert!(at(&buffer, 10, 3).modifier.contains(Modifier::REVERSED));
    assert_eq!(at(&buffer, 8, 3).symbol(), " ", "nothing further left");
    assert_eq!(at(&buffer, 14, 3).symbol(), " ", "nothing outside the area");
}

#[test]
fn no_indicator_is_drawn_at_the_present_or_where_it_does_not_fit() {
    use crate::components::terminal::render::draw_scroll_indicator;
    let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 5));

    draw_scroll_indicator(&mut buffer, Rect::new(0, 0, 20, 5), 0);
    draw_scroll_indicator(&mut buffer, Rect::new(0, 0, 3, 5), 7);

    assert!(buffer.content.iter().all(|cell| cell.symbol() == " "));
}
