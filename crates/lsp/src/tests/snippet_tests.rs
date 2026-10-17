use crate::snippet::{Expanded, expand};

fn plain(snippet: &str) -> Expanded {
    expand(snippet, "")
}

/// The text, and what is selected in it.
fn written(snippet: &str) -> (String, String) {
    let expanded = plain(snippet);
    let selected = expanded.text[expanded.anchor..expanded.cursor].to_owned();
    (expanded.text, selected)
}

/// The text, and where the cursor is when nothing is selected.
fn cursor_in(snippet: &str) -> (String, usize) {
    let expanded = plain(snippet);
    assert_eq!(expanded.anchor, expanded.cursor, "{snippet}");
    (expanded.text, expanded.cursor)
}

#[test]
fn text_without_stops_is_written_as_it_is_with_the_cursor_at_the_end() {
    assert_eq!(cursor_in("push"), ("push".to_owned(), 4));
}

#[test]
fn a_placeholder_is_written_with_its_default_and_selected() {
    let expanded = plain("push(${1:value})");

    assert_eq!(expanded.text, "push(value)");
    assert_eq!((expanded.anchor, expanded.cursor), (5, 10));
}

#[test]
fn a_call_with_no_arguments_has_the_cursor_after_the_brackets() {
    assert_eq!(cursor_in("len()$0"), ("len()".to_owned(), 5));
}

#[test]
fn a_stop_with_no_text_is_a_cursor() {
    assert_eq!(cursor_in("vec![$1]"), ("vec![]".to_owned(), 5));
    assert_eq!(cursor_in("vec![${1}]"), ("vec![]".to_owned(), 5));
    assert_eq!(cursor_in("HashMap<$0>"), ("HashMap<>".to_owned(), 8));
}

#[test]
fn the_lowest_number_wins_whatever_the_order_in_the_text() {
    assert_eq!(
        written("f(${2:b}, ${1:a})"),
        ("f(b, a)".to_owned(), "a".to_owned())
    );
}

#[test]
fn the_final_stop_is_used_only_when_there_is_no_numbered_one() {
    assert_eq!(cursor_in("a$0b$1c"), ("abc".to_owned(), 2));
    assert_eq!(cursor_in("a${0}b"), ("ab".to_owned(), 1));
    assert_eq!(
        written("ne(${1:&self}, ${2:other})$0"),
        ("ne(&self, other)".to_owned(), "&self".to_owned())
    );
}

#[test]
fn a_default_with_nested_placeholders_and_braces_is_written_whole() {
    let snippet = "Foo { ${1:a: ${2:()}}, ${3:b: { 1 }} }";

    assert_eq!(
        written(snippet),
        ("Foo { a: (), b: { 1 } }".to_owned(), "a: ()".to_owned())
    );
}

#[test]
fn a_choice_gives_its_first_alternative() {
    assert_eq!(
        written("x: ${1|u8,u16|}"),
        ("x: u8".to_owned(), "u8".to_owned())
    );
}

#[test]
fn escaped_characters_are_written_as_themselves() {
    assert_eq!(
        written(r"\$1 \} \\ ${1:a\}b}"),
        (r"$1 } \ a}b".to_owned(), "a}b".to_owned())
    );
}

#[test]
fn a_variable_is_its_default_or_nothing_and_a_lone_dollar_stays() {
    assert_eq!(cursor_in("a$TM_FILENAME b${NAME}c").0, "a bc");
    assert_eq!(cursor_in("a${NAME:x}c").0, "axc");
    assert_eq!(cursor_in("cost $ and $"), ("cost $ and $".to_owned(), 12));
}

#[test]
fn every_line_after_the_first_gets_the_indent_of_the_line_it_goes_into() {
    let expanded = expand("if ${1} {\n    $0\n}", "        ");

    assert_eq!(expanded.text, "if  {\n            \n        }");
    assert_eq!((expanded.anchor, expanded.cursor), (3, 3));
}

#[test]
fn an_unclosed_brace_does_not_loop_or_panic() {
    assert_eq!(plain("a${1:b").text, "ab");
    assert_eq!(plain("${").text, "");
    assert_eq!(plain("$").text, "$");
    assert_eq!(plain("${1|a,b").text, "a");
}

#[test]
fn multibyte_text_keeps_the_selection_on_character_boundaries() {
    let expanded = plain("é(${1:ü})ß");

    assert_eq!(expanded.text, "é(ü)ß");
    assert!(expanded.text.is_char_boundary(expanded.anchor));
    assert!(expanded.text.is_char_boundary(expanded.cursor));
    assert_eq!(&expanded.text[expanded.anchor..expanded.cursor], "ü");
}
