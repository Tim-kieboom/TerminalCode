use crate::components::editor::TabWidth;
use crate::components::editor::wrap::{Rows, wrap};

fn tab() -> TabWidth {
    TabWidth::default()
}

/// The rows of `text` as the strings they show.
fn cut(text: &str, width: usize) -> Vec<String> {
    use unicode_segmentation::UnicodeSegmentation;
    let rows = wrap(text, width, tab());
    let graphemes: Vec<&str> = text.graphemes(true).collect();
    (0..rows.count())
        .map(|row| graphemes[rows.start(row)..rows.end(row)].concat())
        .collect()
}

#[test]
fn a_short_line_is_one_row() {
    assert_eq!(cut("hello", 20), vec!["hello"]);
    assert_eq!(wrap("", 20, tab()).count(), 1);
}

#[test]
fn a_row_is_one_cell_narrower_than_the_area() {
    // Room for the cursor after the last character: 10 wide holds 9.
    assert_eq!(cut("123456789", 10), vec!["123456789"]);
    assert_eq!(cut("1234567890", 10), vec!["123456789", "0"]);
}

#[test]
fn the_cut_is_after_the_last_space_that_fits() {
    assert_eq!(cut("hello wide world", 12), vec!["hello wide ", "world"]);
}

#[test]
fn a_word_longer_than_a_row_is_cut_in_the_word() {
    assert_eq!(
        cut("abcdefghijklmnop", 6),
        vec!["abcde", "fghij", "klmno", "p"]
    );
}

#[test]
fn a_word_that_does_not_fit_moves_to_the_next_row_whole() {
    assert_eq!(cut("aa bbbbbbb", 8), vec!["aa ", "bbbbbbb"]);
}

#[test]
fn rows_after_the_first_keep_the_indentation_of_the_line() {
    let rows = wrap("    one two three four", 12, tab());
    assert_eq!(rows.indent_of(0), 0);
    assert_eq!(rows.indent_of(1), 4);
    assert!(rows.count() > 1);
}

#[test]
fn the_indentation_is_at_most_half_the_width() {
    let text = format!("{}word word word", " ".repeat(30));
    let rows = wrap(&text, 20, tab());
    assert_eq!(rows.indent_of(1), 9);
}

#[test]
fn wide_characters_count_two_cells_and_never_split() {
    // Width 7 holds 6 cells: three characters of two cells.
    assert_eq!(
        cut("\u{3053}\u{3053}\u{3053}\u{3053}", 7),
        vec!["\u{3053}\u{3053}\u{3053}", "\u{3053}"]
    );
}

#[test]
fn a_character_wider_than_the_row_still_gets_a_row_of_its_own() {
    let rows = wrap("\u{3053}\u{3053}", 2, tab());
    assert_eq!(rows.count(), 2);
}

#[test]
fn tabs_are_measured_inside_the_row() {
    // A tab at column 0 is 4 wide; 9 cells hold the tab and 5 letters.
    assert_eq!(cut("\tabcdefg", 10), vec!["\tabcde", "fg"]);
}

#[test]
fn combining_marks_stay_with_their_letter() {
    let text = "e\u{301}e\u{301}e\u{301}e\u{301}";
    assert_eq!(wrap(text, 4, tab()).count(), 2);
    assert_eq!(cut(text, 4)[0], "e\u{301}e\u{301}e\u{301}");
}

#[test]
fn every_row_has_a_grapheme_and_the_rows_cover_the_line() {
    for width in 1..30 {
        let text = "the quick  brown fox jumps over the lazy dog";
        let rows = wrap(text, width, tab());
        let joined: String = cut(text, width).concat();
        assert_eq!(joined, text, "width {width}");
        for row in 0..rows.count() {
            assert!(rows.end(row) > rows.start(row), "width {width} row {row}");
        }
    }
}

#[test]
fn row_of_a_column_that_starts_a_row_can_be_the_end_of_the_row_before() {
    let rows = wrap("1234567890", 10, tab());
    assert_eq!(rows.row_of(9, false), 1);
    assert_eq!(rows.row_of(9, true), 0);
    assert_eq!(rows.row_of(5, true), 0);
    assert_eq!(rows.row_of(10, false), 1);
}

#[test]
fn an_unwrapped_line_is_one_row_to_the_end() {
    let rows = Rows::unwrapped(500_000);
    assert!(!rows.is_wrapped());
    assert_eq!(rows.count(), 1);
    assert_eq!(rows.end(0), 500_000);
}
