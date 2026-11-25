use crate::plugins::search::{Query, Subject, tags_to_offer};
use crate::plugins::tags::Tag;

fn tags(names: &[&str]) -> Vec<Tag> {
    names.iter().filter_map(|name| Tag::parse(name)).collect()
}

fn matches(query: &str, id: &str, description: &str, tag_names: &[&str]) -> bool {
    let tags = tags(tag_names);
    Query::parse(query).matches(Subject {
        id,
        name: id,
        description,
        tags: &tags,
    })
}

#[test]
fn an_empty_query_matches_everything() {
    assert!(Query::parse("  ").is_empty());
    assert!(matches("", "git", "", &[]));
}

#[test]
fn a_word_is_found_in_the_name_the_description_and_the_tags_in_any_case() {
    assert!(matches("GIT", "git", "", &[]));
    assert!(matches("status", "pane", "Shows the Status of a repo", &[]));
    assert!(matches("rust", "sol", "", &["syntax", "rust"]));
    assert!(!matches("python", "sol", "", &["syntax", "rust"]));
}

#[test]
fn a_tag_limits_the_list_to_the_plugins_that_have_it() {
    assert!(matches("tag:syntax", "sol", "", &["syntax"]));
    assert!(matches("TAG:Syntax", "sol", "", &["syntax"]));
    assert!(!matches("tag:syntax", "git", "", &["git"]));
}

#[test]
fn every_term_has_to_hold() {
    let have = ["syntax", "rust"];
    assert!(matches("tag:syntax tag:rust", "sol", "", &have));
    assert!(!matches("tag:syntax tag:go", "sol", "", &have));
    assert!(matches("tag:syntax sol", "sol", "", &have));
    assert!(!matches("tag:syntax other", "sol", "", &have));
}

#[test]
fn a_tag_that_is_being_typed_limits_nothing_yet() {
    assert!(matches("tag:", "sol", "", &[]));
    assert!(matches("sol tag:", "sol", "", &[]));
}

#[test]
fn an_unknown_tag_matches_nothing_and_is_said() {
    let query = Query::parse("tag:sytnax tag:git");
    assert_eq!(query.unknown_tags(), ["sytnax"]);
    assert!(!matches("tag:sytnax", "git", "", &["git"]));
    assert!(!matches("tag:sytnax tag:git", "git", "", &["git"]));
}

#[test]
fn c_plus_plus_can_be_asked_for_by_tag() {
    assert!(matches("tag:c++", "clangd", "", &["c++"]));
    assert!(!matches("tag:c", "clangd", "", &["c++"]));
}

fn note(query: &str, nothing_listed: bool) -> Option<String> {
    Query::parse(query).language_note(nothing_listed)
}

#[test]
fn a_language_the_editor_has_is_said_to_be_built_in() {
    assert_eq!(
        note("rust syntax", false).as_deref(),
        Some("Rust syntax is built in")
    );
    assert_eq!(
        note("rust", false).as_deref(),
        Some("Rust syntax is built in")
    );
    assert_eq!(
        note("tag:syntax tag:rust", false).as_deref(),
        Some("Rust syntax is built in")
    );
}

#[test]
fn a_language_is_found_by_its_extensions_too() {
    for query in ["ts", "TypeScript", "tag:typescript"] {
        assert_eq!(
            note(query, false).as_deref(),
            Some("TypeScript syntax is built in"),
            "{query}"
        );
    }
    assert_eq!(
        note("mjs", false).as_deref(),
        Some("JavaScript syntax is built in")
    );
    assert_eq!(
        note("md", false).as_deref(),
        Some("Markdown syntax is built in")
    );
    assert_eq!(
        note("jsonc", false).as_deref(),
        Some("JSON syntax is built in")
    );
}

#[test]
fn every_grammar_of_the_editor_is_found_by_its_name() {
    use crate::syntax::Language;
    for language in [
        Language::Rust,
        Language::Toml,
        Language::Json,
        Language::JavaScript,
        Language::TypeScript,
        Language::Tsx,
        Language::Markdown,
        Language::Nix,
    ] {
        let expected = format!("{} syntax is built in", language.name());
        assert_eq!(note(language.name(), false), Some(expected));
    }
}

#[test]
fn a_language_without_a_grammar_is_not_available_yet_when_nothing_is_listed() {
    assert_eq!(
        note("python syntax", true).as_deref(),
        Some("Python syntax is not available yet")
    );
    assert_eq!(
        note("c++", true).as_deref(),
        Some("C++ syntax is not available yet")
    );
}

#[test]
fn a_language_without_a_grammar_says_nothing_when_a_plugin_is_listed() {
    assert_eq!(note("python syntax", false), None);
}

#[test]
fn a_search_that_is_about_something_else_has_no_note() {
    assert_eq!(note("", true), None);
    assert_eq!(note("tag:syntax", true), None);
    assert_eq!(note("rust git", false), None);
    assert_eq!(note("tag:git rust", false), None);
    assert_eq!(note("tag:sytnax rust", false), None);
    assert_eq!(note("status", true), None);
}

#[test]
fn several_languages_are_each_told_about() {
    assert_eq!(
        note("rust python", true).as_deref(),
        Some("Rust syntax is built in; Python syntax is not available yet")
    );
}

#[test]
fn the_tags_offered_are_all_of_them_until_a_tag_is_typed() {
    assert_eq!(tags_to_offer("").len(), Tag::all().len());
    assert_eq!(tags_to_offer("rust ").len(), Tag::all().len());
    let names: Vec<&str> = tags_to_offer("tag:c")
        .iter()
        .map(|tag| tag.name())
        .collect();
    assert_eq!(names, ["c", "c++", "c#", "css"]);
    assert!(tags_to_offer("tag:zzz").is_empty());
}
