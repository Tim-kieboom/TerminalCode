use std::path::Path;

use ratatui::style::Color;
use ropey::Rope;

use crate::syntax::{Highlighter, Language};
use crate::ui::theme::Theme;

/// A theme that gives every kind of token a color of its own.
const THEME: &str = r##"
["syntax.keyword"]
text = "red"
["syntax.string"]
text = "green"
["syntax.string.special.key"]
text = "blue"
["syntax.number"]
text = "yellow"
["syntax.boolean"]
text = "magenta"
["syntax.constant"]
text = "magenta"
["syntax.comment"]
text = "gray"
["syntax.property"]
text = "blue"
["syntax.type"]
text = "cyan"
["syntax.text.title"]
text = "cyan"
["syntax.text.strong"]
text = "red"
["syntax.text.emphasis"]
text = "green"
["syntax.text.literal"]
text = "yellow"
["syntax.text.reference"]
text = "blue"
["syntax.text.uri"]
text = "magenta"
"##;

/// The colored words of `text` and their colors, in order.
fn colors(language: Language, text: &str) -> Vec<(String, Color)> {
    colors_in(language, text, 0..text.len())
}

fn colors_in(
    language: Language,
    text: &str,
    range: std::ops::Range<usize>,
) -> Vec<(String, Color)> {
    let theme = Theme::from_toml(THEME).unwrap();
    let mut highlighter = Highlighter::new(language, &theme).unwrap();
    let rope = Rope::from_str(text);
    highlighter.parse(&rope).unwrap();
    highlighter
        .spans(&rope, range)
        .into_iter()
        .filter_map(|span| Some((text[span.range].to_owned(), span.style.fg?)))
        .collect()
}

fn has(colors: &[(String, Color)], word: &str, color: Color) -> bool {
    colors.iter().any(|(text, c)| text == word && *c == color)
}

#[test]
fn the_language_comes_from_the_extension() {
    let language = |path: &str| Language::from_path(Path::new(path));

    assert_eq!(language("src/main.rs"), Some(Language::Rust));
    assert_eq!(language("Cargo.toml"), Some(Language::Toml));
    assert_eq!(language("a/b.json"), Some(Language::Json));
    assert_eq!(language("README.md"), Some(Language::Markdown));
    assert_eq!(language("notes.markdown"), Some(Language::Markdown));
    assert_eq!(language("flake.nix"), Some(Language::Nix));
    assert_eq!(language("notes.txt"), None);
    assert_eq!(language("Makefile"), None);
}

#[test]
fn every_language_sets_up_with_the_default_theme() {
    for language in [
        Language::Rust,
        Language::Toml,
        Language::Json,
        Language::Markdown,
        Language::Nix,
    ] {
        assert!(
            Highlighter::new(language, &Theme::default()).is_ok(),
            "{}",
            language.name()
        );
    }
}

#[test]
fn json_keys_strings_numbers_and_constants_are_colored() {
    let found = colors(Language::Json, "{\"a\": [1, true, null, \"x\"]}");

    assert!(has(&found, "\"a\"", Color::Blue), "{found:?}");
    assert!(has(&found, "1", Color::Yellow), "{found:?}");
    assert!(has(&found, "true", Color::Magenta), "{found:?}");
    assert!(has(&found, "null", Color::Magenta), "{found:?}");
    assert!(has(&found, "\"x\"", Color::Green), "{found:?}");
}

#[test]
fn toml_keys_strings_numbers_booleans_and_comments_are_colored() {
    let found = colors(
        Language::Toml,
        "[package]\nname = \"x\" # the name\nn = 3\nok = true\n",
    );

    // This grammar's query colors keys as `type`.
    assert!(has(&found, "name", Color::Cyan), "{found:?}");
    assert!(has(&found, "package", Color::Cyan), "{found:?}");
    assert!(has(&found, "\"x\"", Color::Green), "{found:?}");
    assert!(has(&found, "# the name", Color::Gray), "{found:?}");
    assert!(has(&found, "3", Color::Yellow), "{found:?}");
    assert!(has(&found, "true", Color::Magenta), "{found:?}");
}

#[test]
fn nix_keywords_strings_numbers_and_comments_are_colored() {
    let found = colors(
        Language::Nix,
        "# top\n{ pkgs }: let x = 1; in \"hi ${x}\"\n",
    );

    assert!(has(&found, "# top", Color::Gray), "{found:?}");
    assert!(has(&found, "let", Color::Red), "{found:?}");
    assert!(has(&found, "in", Color::Red), "{found:?}");
    assert!(has(&found, "1", Color::Yellow), "{found:?}");
    assert!(found.iter().any(|(_, c)| *c == Color::Green), "{found:?}");
}

const MARKDOWN: &str = "# Title\n\nSome **bold**, *slanted* and `code` text with a [link](http://x.y).\n\nSecond paragraph.\n";

#[test]
fn markdown_headings_are_colored_by_the_block_grammar() {
    let found = colors(Language::Markdown, MARKDOWN);

    assert!(has(&found, "Title", Color::Cyan), "{found:?}");
}

#[test]
fn markdown_paragraphs_are_colored_by_the_inline_grammar() {
    let found = colors(Language::Markdown, MARKDOWN);

    assert!(has(&found, "**bold**", Color::Red), "{found:?}");
    assert!(has(&found, "*slanted*", Color::Green), "{found:?}");
    assert!(has(&found, "`code`", Color::Yellow), "{found:?}");
    assert!(has(&found, "link", Color::Blue), "{found:?}");
    assert!(has(&found, "http://x.y", Color::Magenta), "{found:?}");
}

#[test]
fn markdown_inline_spans_are_only_made_for_the_range_asked() {
    let start = MARKDOWN.find("Second").unwrap();

    let found = colors_in(Language::Markdown, MARKDOWN, start..MARKDOWN.len());

    assert!(!has(&found, "**bold**", Color::Red), "{found:?}");
    assert!(!has(&found, "Title", Color::Cyan), "{found:?}");
}

#[test]
fn markdown_inline_spans_are_cut_at_the_edges_of_the_range() {
    let start = MARKDOWN.find("bold").unwrap();

    let found = colors_in(Language::Markdown, MARKDOWN, start..start + 4);

    assert!(has(&found, "bold", Color::Red), "{found:?}");
}

#[test]
fn markdown_without_any_inline_text_is_fine() {
    assert!(colors(Language::Markdown, "\n\n").is_empty());
    assert!(colors(Language::Markdown, "").is_empty());
}

#[test]
fn markdown_spans_never_overlap() {
    let theme = Theme::from_toml(THEME).unwrap();
    let mut highlighter = Highlighter::new(Language::Markdown, &theme).unwrap();
    let rope = Rope::from_str(MARKDOWN);
    highlighter.parse(&rope).unwrap();

    let spans = highlighter.spans(&rope, 0..MARKDOWN.len());

    for pair in spans.windows(2) {
        assert!(pair[0].range.end <= pair[1].range.start, "{pair:?}");
    }
}
