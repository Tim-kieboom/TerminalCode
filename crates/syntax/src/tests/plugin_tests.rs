//! Languages a plugin adds, with the grammar of the Sol extension for Zed
//! (`fixtures/sol.wasm`, built with `tree-sitter build --wasm`) and its highlight query.
//! The table is shared by the whole program, so each test has an owner and extensions of its own.

use std::path::Path;

use ratatui::style::Color;
use ropey::Rope;

use super::TestTheme;
use crate::{
    Comment, Highlighter, Language, LanguageSpec, SyntaxError, register_language,
    unregister_language,
};

const GRAMMAR: &[u8] = include_bytes!("fixtures/sol.wasm");
const HIGHLIGHTS: &str = include_str!("fixtures/sol-highlights.scm");

fn spec(extension: &str) -> LanguageSpec {
    LanguageSpec {
        name: "Sol".to_owned(),
        extensions: vec![extension.to_owned()],
        comment: Some("//".to_owned()),
        entry: "sol".to_owned(),
        grammar: GRAMMAR.to_vec(),
        highlights: HIGHLIGHTS.to_owned(),
    }
}

fn language(file: &str) -> Option<Language> {
    Language::from_path(Path::new(file))
}

fn colors(language: Language, source: &str) -> Vec<(String, Option<Color>)> {
    let mut highlighter = Highlighter::new(language, &TestTheme::ordinary()).unwrap();
    let text = Rope::from_str(source);
    highlighter.parse(&text).unwrap();
    highlighter
        .spans(&text, 0..source.len())
        .iter()
        .map(|span| (source[span.range.clone()].to_owned(), span.style.fg))
        .collect()
}

#[test]
fn files_of_a_plugins_language_are_that_language_until_it_is_taken_away() {
    register_language("test-a", spec("sola")).unwrap();

    let found = language("main.sola").expect("a language");
    assert_eq!(found.name(), "Sol");
    assert_eq!(language("main.solb"), None, "another extension");

    unregister_language("test-a");

    assert_eq!(language("main.sola"), None);
}

#[test]
fn the_languages_built_in_come_first_so_a_plugin_cannot_take_rs_files() {
    let mut taking = spec("rs");
    taking.extensions.push("solc1".to_owned());
    register_language("test-b", taking).unwrap();

    assert_eq!(language("lib.rs"), Some(Language::Rust));
    assert!(matches!(language("a.solc1"), Some(Language::Plugin(_))));
    unregister_language("test-b");
}

#[test]
fn the_grammar_of_a_plugin_colors_the_code_by_its_query() {
    register_language("test-c", spec("solc2")).unwrap();
    let sol = language("a.solc2").unwrap();

    let found = colors(
        sol,
        "// top\npub struct Point { x: i32 }\nfn main() { println(\"hi\", 3); if true { return; } }\n",
    );

    let has = |text: &str, color: Color| found.contains(&(text.to_owned(), Some(color)));
    assert!(has("// top", Color::Gray), "{found:?}");
    assert!(has("pub", Color::Red), "{found:?}");
    assert!(has("struct", Color::Red), "{found:?}");
    assert!(has("if", Color::Red), "{found:?}");
    assert!(has("return", Color::Red), "{found:?}");
    assert!(has("i32", Color::Cyan), "{found:?}");
    assert!(has("\"hi\"", Color::Green), "{found:?}");
    assert!(has("3", Color::Yellow), "{found:?}");
    assert!(has("println", Color::Red), "a keyword of Sol: {found:?}");
    assert!(has("main", Color::Blue), "a call is a function: {found:?}");
    unregister_language("test-c");
}

#[test]
fn text_is_reparsed_after_an_edit_with_the_grammar_of_a_plugin() {
    register_language("test-d", spec("solc3")).unwrap();
    let sol = language("a.solc3").unwrap();
    let mut highlighter = Highlighter::new(sol, &TestTheme::ordinary()).unwrap();
    highlighter.parse(&Rope::from_str("pub x\n")).unwrap();

    highlighter
        .parse(&Rope::from_str("pub x\nreturn\n"))
        .unwrap();

    let text = Rope::from_str("pub x\nreturn\n");
    assert_eq!(highlighter.spans(&text, 0..text.len_bytes()).len(), 2);
    unregister_language("test-d");
}

#[test]
fn a_language_with_a_comment_marker_comments_lines_and_one_without_does_not() {
    register_language("test-e", spec("solc4")).unwrap();
    let mut plain = spec("solc5");
    plain.comment = None;
    register_language("test-f", plain).unwrap();

    assert_eq!(
        language("a.solc4").unwrap().comment(),
        Some(Comment::Line("//"))
    );
    assert_eq!(language("a.solc5").unwrap().comment(), None);
    unregister_language("test-e");
    unregister_language("test-f");
}

#[test]
fn a_grammar_that_is_not_one_or_a_query_that_is_not_valid_is_refused_and_changes_nothing() {
    let mut broken = spec("solc6");
    broken.grammar = b"not a module".to_vec();
    assert!(matches!(
        register_language("test-g", broken),
        Err(SyntaxError::Wasm { .. })
    ));
    let mut wrong_entry = spec("solc6");
    wrong_entry.entry = "no_such_language".to_owned();
    assert!(matches!(
        register_language("test-g", wrong_entry),
        Err(SyntaxError::Wasm { .. })
    ));
    let mut bad_query = spec("solc6");
    bad_query.highlights = "(no_such_node) @keyword".to_owned();
    assert!(matches!(
        register_language("test-g", bad_query),
        Err(SyntaxError::Query { .. })
    ));

    assert_eq!(language("a.solc6"), None);
}

#[test]
fn a_language_that_was_registered_again_or_taken_away_is_not_the_one_a_document_had() {
    register_language("test-h", spec("solc7")).unwrap();
    let first = language("a.solc7").unwrap();
    register_language("test-h", spec("solc7")).unwrap();
    let second = language("a.solc7").unwrap();

    assert_ne!(first, second, "documents set themselves up again");
    assert!(matches!(
        Highlighter::new(first, &TestTheme::ordinary()),
        Err(SyntaxError::Gone)
    ));
    assert!(Highlighter::new(second, &TestTheme::ordinary()).is_ok());
    unregister_language("test-h");
    assert!(matches!(
        Highlighter::new(second, &TestTheme::ordinary()),
        Err(SyntaxError::Gone)
    ));
}

#[test]
fn where_two_patterns_capture_the_same_text_the_first_in_the_query_wins() {
    // A call is a function, not a variable: the plugin's query has the call patterns before
    // `(identifier) @variable`, as the editor takes the first (Zed takes the last).
    register_language("test-i", spec("solc8")).unwrap();
    let theme = TestTheme::from_toml(
        "[\"syntax.function\"]\ntext = \"blue\"\n[\"syntax.variable\"]\ntext = \"white\"\n",
    );
    let mut highlighter = Highlighter::new(language("a.solc8").unwrap(), &theme).unwrap();
    let source = "main() other\n";
    let text = Rope::from_str(source);
    highlighter.parse(&text).unwrap();

    let found: Vec<(String, Option<Color>)> = highlighter
        .spans(&text, 0..source.len())
        .iter()
        .map(|span| (source[span.range.clone()].to_owned(), span.style.fg))
        .collect();

    assert!(
        found.contains(&("main".to_owned(), Some(Color::Blue))),
        "{found:?}"
    );
    assert!(
        found.contains(&("other".to_owned(), Some(Color::White))),
        "{found:?}"
    );
    unregister_language("test-i");
}
