use ratatui::style::Style;

use crate::buffer::{Buffer, Edit};
use crate::syntax::Span;
use crate::syntax::mapping::map_spans;

/// The `EditInfo` of replacing `range` of `text` with `with`, as the buffer
/// reports it.
fn edit_info(text: &str, range: std::ops::Range<usize>, with: &str) -> crate::buffer::EditInfo {
    let mut buffer = Buffer::from_text(text);
    let mut transaction = buffer.begin_transaction(Default::default());
    transaction.apply(&Edit::new(range, with)).unwrap();
    transaction.commit(Default::default());
    buffer.take_edit_log().pop().unwrap()
}

fn spans(ranges: &[(usize, usize)]) -> Vec<Span> {
    ranges
        .iter()
        .map(|&(start, end)| Span {
            range: start..end,
            style: Style::default(),
        })
        .collect()
}

fn ranges(spans: &[Span]) -> Vec<(usize, usize)> {
    spans
        .iter()
        .map(|span| (span.range.start, span.range.end))
        .collect()
}

/// Maps `(start, end)` spans through replacing `range` with `with`; the text
/// is only as long as the offsets need.
fn mapped(
    list: &[(usize, usize)],
    range: std::ops::Range<usize>,
    with: &str,
) -> Vec<(usize, usize)> {
    let text = "x".repeat(40);
    let mut list = spans(list);
    map_spans(&mut list, &edit_info(&text, range, with));
    ranges(&list)
}

#[test]
fn an_edit_after_a_span_leaves_it_alone() {
    assert_eq!(mapped(&[(2, 5)], 10..12, "abc"), [(2, 5)]);
}

#[test]
fn an_edit_before_a_span_shifts_it() {
    assert_eq!(mapped(&[(10, 15)], 2..2, "abc"), [(13, 18)]);
    assert_eq!(mapped(&[(10, 15)], 2..6, ""), [(6, 11)]);
}

#[test]
fn an_insert_exactly_at_the_end_of_a_span_stays_outside_it() {
    assert_eq!(mapped(&[(2, 5)], 5..5, "abc"), [(2, 5)]);
}

#[test]
fn an_insert_exactly_at_the_start_of_a_span_stays_outside_it() {
    assert_eq!(mapped(&[(2, 5)], 2..2, "abc"), [(5, 8)]);
}

#[test]
fn an_insert_inside_a_span_grows_it() {
    assert_eq!(mapped(&[(2, 8)], 5..5, "abc"), [(2, 11)]);
}

#[test]
fn a_delete_inside_a_span_shrinks_it() {
    assert_eq!(mapped(&[(2, 12)], 5..8, ""), [(2, 9)]);
}

#[test]
fn replacing_a_whole_span_gives_the_new_text_its_color() {
    assert_eq!(mapped(&[(2, 5)], 2..5, "abcdef"), [(2, 8)]);
}

#[test]
fn deleting_a_whole_span_drops_it() {
    assert_eq!(mapped(&[(2, 5), (8, 10)], 2..5, ""), [(5, 7)]);
}

#[test]
fn an_edit_that_covers_the_tail_of_a_span_trims_it_and_keeps_the_new_text_out() {
    assert_eq!(mapped(&[(2, 10)], 6..14, "abc"), [(2, 6)]);
}

#[test]
fn an_edit_that_covers_the_head_of_a_span_trims_it() {
    assert_eq!(mapped(&[(6, 14)], 2..10, "abc"), [(5, 9)]);
}

#[test]
fn an_edit_that_covers_several_spans_drops_those_inside_it_and_trims_the_edges() {
    let result = mapped(&[(0, 4), (5, 7), (8, 12), (14, 16)], 2..10, "");
    assert_eq!(result, [(0, 2), (2, 4), (6, 8)]);
}

#[test]
fn spans_stay_in_order_and_apart() {
    let result = mapped(&[(0, 3), (3, 6), (6, 9)], 3..3, "zz");
    assert_eq!(result, [(0, 3), (5, 8), (8, 11)]);
}
