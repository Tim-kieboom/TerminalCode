use super::*;

fn screen() -> vt100::Parser {
    vt100::Parser::new(24, 80, 0)
}

fn text(screen: &vt100::Parser) -> String {
    screen.screen().contents()
}

#[test]
fn output_without_a_query_gets_no_answer() {
    let mut screen = screen();
    let mut replier = Replier::default();

    let replies = replier.feed(&mut screen, b"hello");

    assert!(replies.is_empty());
    assert_eq!(text(&screen), "hello");
}

#[test]
fn a_query_at_the_very_start_is_answered_with_the_top_left_corner() {
    let mut screen = screen();
    let mut replier = Replier::default();

    let replies = replier.feed(&mut screen, b"\x1b[6n");

    assert_eq!(replies, b"\x1b[1;1R");
}

#[test]
fn the_answer_counts_from_one_and_follows_the_text() {
    let mut screen = screen();
    let mut replier = Replier::default();

    let replies = replier.feed(&mut screen, b"abc\x1b[6n");

    assert_eq!(replies, b"\x1b[1;4R");
}

#[test]
fn the_answer_follows_line_breaks() {
    let mut screen = screen();
    let mut replier = Replier::default();

    let replies = replier.feed(&mut screen, b"a\r\nb\x1b[6n");

    assert_eq!(replies, b"\x1b[2;2R");
}

#[test]
fn the_answer_is_for_where_the_cursor_was_at_the_query_not_after_the_rest() {
    let mut screen = screen();
    let mut replier = Replier::default();

    let replies = replier.feed(&mut screen, b"ab\x1b[6ncdef");

    assert_eq!(replies, b"\x1b[1;3R");
    assert_eq!(text(&screen), "abcdef");
}

#[test]
fn every_query_is_answered() {
    let mut screen = screen();
    let mut replier = Replier::default();

    let replies = replier.feed(&mut screen, b"\x1b[6nab\x1b[6n");

    assert_eq!(replies, b"\x1b[1;1R\x1b[1;3R");
}

#[test]
fn a_query_split_over_two_outputs_is_answered_once_it_is_whole() {
    for split in 1..CURSOR_QUERY.len() {
        let mut screen = screen();
        let mut replier = Replier::default();

        let first = replier.feed(&mut screen, &CURSOR_QUERY[..split]);
        let second = replier.feed(&mut screen, &CURSOR_QUERY[split..]);

        assert!(first.is_empty(), "split at {split}");
        assert_eq!(second, b"\x1b[1;1R", "split at {split}");
    }
}

#[test]
fn text_before_a_split_query_is_shown_at_once() {
    let mut screen = screen();
    let mut replier = Replier::default();

    replier.feed(&mut screen, b"abc\x1b[");

    assert_eq!(text(&screen), "abc");
}

#[test]
fn bytes_held_for_a_query_that_never_came_are_not_lost() {
    let mut screen = screen();
    let mut replier = Replier::default();

    let first = replier.feed(&mut screen, b"a\x1b[");
    let second = replier.feed(&mut screen, b"31mred");

    assert!(first.is_empty() && second.is_empty());
    assert_eq!(text(&screen), "ared");
    assert_eq!(
        screen.screen().cell(0, 1).map(|cell| cell.fgcolor()),
        Some(vt100::Color::Idx(1))
    );
}

#[test]
fn other_escape_sequences_are_left_alone() {
    let mut screen = screen();
    let mut replier = Replier::default();

    let replies = replier.feed(&mut screen, b"\x1b[31mred\x1b[0m \x1b[5n");

    assert!(replies.is_empty());
    assert_eq!(text(&screen), "red ");
}
