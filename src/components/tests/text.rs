use std::path::Path;

use ratatui::layout::{Position, Rect};

use crate::components::text::{
    fit, head_fitting, place, short_path, truncate, wrap_indented, wrap_text, wrap_words,
};

#[test]
fn truncate_cuts_to_the_columns_and_never_splits_a_wide_character() {
    assert_eq!(truncate("abcdef", 3), "abc");
    assert_eq!(truncate("\u{3053}\u{3053}", 3), "\u{3053}");
    assert_eq!(truncate("ab", 10), "ab");
    assert_eq!(truncate("ab", 0), "");
}

#[test]
fn head_fitting_borrows_the_start_that_fits() {
    assert_eq!(head_fitting("hello", 3), "hel");
    assert_eq!(head_fitting("hello", 9), "hello");
}

#[test]
fn fit_marks_the_cut_with_an_ellipsis() {
    assert_eq!(fit("hello", 5), "hello");
    assert_eq!(fit("hello world", 6), "hello…");
    assert_eq!(fit("hello", 0), "…");
}

#[test]
fn wrap_text_keeps_indent_and_breaks_long_words() {
    assert_eq!(wrap_text("  indented words", 40), ["  indented words"]);
    assert_eq!(wrap_text("abcdefghij", 4), ["abcd", "efgh", "ij"]);
    assert_eq!(wrap_text("one\ntwo", 20), ["one", "two"]);
}

#[test]
fn wrap_words_joins_words_with_one_space_and_loses_the_indent() {
    assert_eq!(wrap_words("aa   bb cc", 5), ["aa bb", "cc"]);
    assert_eq!(wrap_words("  abcdefgh", 3), ["abc", "def", "gh"]);
    assert_eq!(wrap_words("", 5), [""]);
    assert_eq!(wrap_words("one\n\ntwo", 9), ["one", "", "two"]);
}

#[test]
fn wrap_indented_keeps_the_indent_and_drops_the_space_at_a_break() {
    assert_eq!(wrap_indented("aa bb cc", 5), ["aa bb", "cc"]);
    assert_eq!(wrap_indented("  - item", 20), ["  - item"]);
    assert_eq!(wrap_indented("", 5), [""]);
}

#[test]
fn short_path_is_relative_to_the_project_or_the_last_three_parts() {
    let project = Path::new("/work/project");
    assert_eq!(
        short_path(Path::new("/work/project/src/lib.rs"), project),
        "src/lib.rs"
    );
    assert_eq!(
        short_path(Path::new("/a/b/c/d/e.rs"), project),
        "…/c/d/e.rs"
    );
    assert_eq!(short_path(Path::new("c/d.rs"), project), "c/d.rs");
}

#[test]
fn place_goes_below_the_cursor_above_at_the_bottom_and_stays_inside() {
    let screen = Rect::new(0, 0, 80, 24);
    let below = place(screen, Position::new(10, 5), 20, 4);
    assert_eq!((below.x, below.y), (10, 6));
    let above = place(screen, Position::new(10, 22), 20, 4);
    assert_eq!(above.y, 18);
    let right_edge = place(screen, Position::new(75, 5), 20, 4);
    assert_eq!(right_edge.x, 60);
}
