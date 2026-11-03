use super::{Alignment, Side};
use crate::git::hash_lines;

fn align(old: &str, new: &str) -> Alignment {
    Alignment::new(&hash_lines(old), &hash_lines(new))
}

/// What each row shows: the old line and the new line, `-` for a filler.
fn rows(alignment: &Alignment) -> Vec<(String, String)> {
    let show = |side, row| {
        alignment
            .line_at(side, row)
            .map_or("-".to_owned(), |line| line.to_string())
    };
    (0..alignment.rows())
        .map(|row| (show(Side::Old, row), show(Side::New, row)))
        .collect()
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

#[test]
fn texts_that_are_the_same_line_up_row_for_row() {
    let alignment = align("a\nb\n", "a\nb\n");

    assert!(alignment.is_same());
    assert_eq!(
        rows(&alignment),
        pairs(&[("0", "0"), ("1", "1"), ("2", "2")])
    );
}

#[test]
fn a_removed_line_leaves_a_filler_row_on_the_new_side() {
    let alignment = align("a\nb\nc\n", "a\nc\n");

    assert_eq!(
        rows(&alignment),
        pairs(&[("0", "0"), ("1", "-"), ("2", "1"), ("3", "2")])
    );
}

#[test]
fn an_added_line_leaves_a_filler_row_on_the_old_side() {
    let alignment = align("a\nc\n", "a\nb\nc\n");

    assert_eq!(
        rows(&alignment),
        pairs(&[("0", "0"), ("-", "1"), ("1", "2"), ("2", "3")])
    );
}

#[test]
fn a_changed_stretch_pairs_its_lines_and_pads_the_shorter_side() {
    // Two lines became three: two pairs and one filler on the old side.
    let alignment = align("a\nx\ny\nz\n", "a\n1\n2\n3\nz\n");

    assert_eq!(
        rows(&alignment),
        pairs(&[
            ("0", "0"),
            ("1", "1"),
            ("2", "2"),
            ("-", "3"),
            ("3", "4"),
            ("4", "5"),
        ])
    );
    assert!(!alignment.is_changed(0));
    assert!(alignment.is_changed(1) && alignment.is_changed(3));
    assert!(!alignment.is_changed(4));
}

#[test]
fn a_line_is_found_in_its_row_and_a_row_in_its_line() {
    let alignment = align("a\nb\nc\nd\n", "a\nc\nd\ne\nf\n");

    for side in [Side::Old, Side::New] {
        for row in 0..alignment.rows() {
            if let Some(line) = alignment.line_at(side, row) {
                assert_eq!(alignment.row_of(side, line), row, "{side:?} row {row}");
            }
        }
    }
}

#[test]
fn a_filler_row_uses_the_nearest_line_above_it() {
    let alignment = align("a\nb\nc\n", "a\nc\n");

    // Row 1 is a filler on the new side: line 0 is above it.
    assert_eq!(alignment.line_at(Side::New, 1), None);
    assert_eq!(alignment.nearest_line(Side::New, 1), 0);
    assert_eq!(alignment.nearest_line(Side::New, 2), 1);
}

#[test]
fn rows_past_the_text_have_no_line() {
    let alignment = align("a\n", "a\nb\n");

    assert_eq!(alignment.line_at(Side::New, 99), None);
    assert_eq!(alignment.line_at(Side::Old, 99), None);
}

#[test]
fn the_row_of_the_line_after_the_last_is_the_row_after_the_last() {
    // The old text has 3 lines (`a`, `b` and the empty one), the new text 2.
    let alignment = align("a\nb\n", "a\n");

    assert_eq!(alignment.rows(), 3);
    assert_eq!(alignment.row_of(Side::New, 2), alignment.rows());
    assert_eq!(alignment.row_of(Side::Old, 3), alignment.rows());
}

#[test]
fn a_new_file_has_nothing_on_the_old_side() {
    // Nothing in git: the old text has no lines at all.
    let alignment = Alignment::new(&[], &hash_lines("a\nb\n"));

    assert_eq!(alignment.line_at(Side::Old, 0), None);
    assert_eq!(alignment.line_at(Side::New, 2), Some(2));
    assert_eq!(alignment.rows(), 3);
    assert!((0..3).all(|row| alignment.is_changed(row)));
}

#[test]
fn the_next_and_previous_change_are_where_a_change_starts() {
    // Changes at line 1 (a modification) and line 5 (a removal before it).
    let alignment = align("a\nb\nc\nd\ne\nf\ng\n", "a\nB\nc\nd\ne\ng\n");

    assert_eq!(alignment.next_change(0), Some(1));
    assert_eq!(alignment.next_change(1), Some(5));
    assert_eq!(alignment.next_change(5), None);
    assert_eq!(alignment.previous_change(5), Some(1));
    assert_eq!(alignment.previous_change(1), None);
    assert_eq!(alignment.previous_change(6), Some(5));
}

#[test]
fn a_removal_at_the_very_end_is_shown_at_the_last_line() {
    let alignment = align("a\nb\nc", "a\nb");

    let next = alignment.next_change(0).unwrap();
    assert!(next <= 1, "held to the text: {next}");
}
