use crate::components::finder::target::{Parsed, parse};

fn parsed(text: &str, line: Option<usize>, column: Option<usize>) -> Parsed<'_> {
    Parsed { text, line, column }
}

#[test]
fn a_query_with_no_place_is_all_text() {
    assert_eq!(parse("main.rs"), parsed("main.rs", None, None));
    assert_eq!(parse(""), parsed("", None, None));
}

#[test]
fn a_colon_and_a_number_name_the_line() {
    assert_eq!(parse("main.rs:42"), parsed("main.rs", Some(42), None));
}

#[test]
fn a_second_number_names_the_column() {
    assert_eq!(parse("main.rs:42:7"), parsed("main.rs", Some(42), Some(7)));
}

#[test]
fn a_colon_after_the_line_is_dropped() {
    assert_eq!(parse("main.rs:42:"), parsed("main.rs", Some(42), None));
}

#[test]
fn a_lone_trailing_colon_is_dropped_from_the_text() {
    assert_eq!(parse("main.rs:"), parsed("main.rs", None, None));
}

#[test]
fn a_number_in_a_name_is_not_a_line() {
    assert_eq!(parse("foo2"), parsed("foo2", None, None));
    assert_eq!(parse("v1.2"), parsed("v1.2", None, None));
}

#[test]
fn a_place_alone_is_just_text() {
    assert_eq!(parse(":42"), parsed(":42", None, None));
}

#[test]
fn places_count_from_one_and_a_zero_is_a_one() {
    assert_eq!(parse("main.rs:0"), parsed("main.rs", Some(1), None));
    assert_eq!(parse("main.rs:3:0"), parsed("main.rs", Some(3), Some(1)));
}

#[test]
fn something_that_is_not_a_number_after_a_colon_stays_text() {
    assert_eq!(parse("a:b"), parsed("a:b", None, None));
    assert_eq!(parse("main.rs:4x"), parsed("main.rs:4x", None, None));
}

#[test]
fn a_huge_number_is_text_not_a_crash() {
    let query = format!("main.rs:{}", "9".repeat(40));
    assert_eq!(parse(&query).line, None);
}

#[test]
fn trailing_spaces_do_not_hide_the_place() {
    assert_eq!(parse("main.rs:42  "), parsed("main.rs", Some(42), None));
}
