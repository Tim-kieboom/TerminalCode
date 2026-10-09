use std::ops::Range;

use crate::syntax::merge_ranges;

#[test]
fn overlapping_and_touching_ranges_are_joined() {
    assert_eq!(
        merge_ranges(vec![0..5, 3..8, 8..10, 20..30], 8),
        [0..10, 20..30]
    );
}

#[test]
fn ranges_come_out_sorted() {
    assert_eq!(merge_ranges(vec![20..30, 0..5], 8), [0..5, 20..30]);
}

#[test]
fn empty_ranges_are_dropped() {
    assert_eq!(
        merge_ranges(vec![4..4, Range { start: 9, end: 2 }, 0..1], 8),
        vec![0..1]
    );
}

#[test]
fn a_range_inside_another_disappears_into_it() {
    assert_eq!(merge_ranges(vec![0..100, 10..20], 8), vec![0..100]);
}

#[test]
fn at_most_the_cap_is_kept_the_first_by_position() {
    let many: Vec<_> = (0..12).map(|n| n * 10..n * 10 + 5).collect();

    let merged = merge_ranges(many, 8);

    assert_eq!(merged.len(), 8);
    assert_eq!(merged[0], 0..5);
    assert_eq!(merged[7], 70..75);
}
