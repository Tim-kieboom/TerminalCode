use std::path::Path;

use ratatui::style::Color;
use ropey::Rope;

use super::TestTheme as Theme;
use crate::{Highlighter, Language};

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
    let theme = Theme::from_toml(THEME);
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
    for name in ["a.js", "a.mjs", "a.cjs", "App.jsx"] {
        assert_eq!(language(name), Some(Language::JavaScript), "{name}");
    }
    for name in ["a.ts", "a.mts", "a.cts"] {
        assert_eq!(language(name), Some(Language::TypeScript), "{name}");
    }
    assert_eq!(language("App.tsx"), Some(Language::Tsx));
    assert_eq!(language("README.md"), Some(Language::Markdown));
    assert_eq!(language("notes.markdown"), Some(Language::Markdown));
    assert_eq!(language("flake.nix"), Some(Language::Nix));
    assert_eq!(language("notes.txt"), None);
    assert_eq!(language("Makefile"), None);
}

#[test]
fn every_language_sets_up_with_each_built_in_theme() {
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
        assert!(
            Highlighter::new(language, &Theme::ordinary()).is_ok(),
            "{}",
            language.name()
        );
        assert!(
            Highlighter::new(language, &Theme::ordinary()).is_ok(),
            "light {}",
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
    let theme = Theme::from_toml(THEME);
    let mut highlighter = Highlighter::new(Language::Markdown, &theme).unwrap();
    let rope = Rope::from_str(MARKDOWN);
    highlighter.parse(&rope).unwrap();

    let spans = highlighter.spans(&rope, 0..MARKDOWN.len());

    for pair in spans.windows(2) {
        assert!(pair[0].range.end <= pair[1].range.start, "{pair:?}");
    }
}

#[test]
fn javascript_functions_methods_and_constructors_are_not_taken_for_variables() {
    let theme = Theme::from_toml(
        "[\"syntax.function\"]\ntext = \"yellow\"\n[\"syntax.variable\"]\ntext = \"white\"\n[\"syntax.constructor\"]\ntext = \"cyan\"\n",
    );
    let source = "function go() {}\nconst run = () => 1;\nlet x = new Map();\ngo();\nx.size();\nclass A { m() {} }\n";
    let mut highlighter = Highlighter::new(Language::JavaScript, &theme).unwrap();
    let rope = Rope::from_str(source);
    highlighter.parse(&rope).unwrap();
    let found: Vec<(String, Color)> = highlighter
        .spans(&rope, 0..source.len())
        .into_iter()
        .filter_map(|span| Some((source[span.range].to_owned(), span.style.fg?)))
        .collect();

    for function in ["go", "run", "size", "m"] {
        assert!(
            has(&found, function, Color::Yellow),
            "{function}: {found:?}"
        );
    }
    assert!(has(&found, "Map", Color::Cyan), "a constructor: {found:?}");
    assert!(has(&found, "x", Color::White), "a variable: {found:?}");
}

/// What each of the colors in the TypeScript tests stands for.
const TYPESCRIPT_THEME: &str = "[\"syntax.keyword\"]\ntext = \"red\"\n[\"syntax.type\"]\ntext = \"cyan\"\n[\"syntax.function\"]\ntext = \"yellow\"\n[\"syntax.constant\"]\ntext = \"magenta\"\n[\"syntax.attribute\"]\ntext = \"green\"\n[\"syntax.variable\"]\ntext = \"white\"\n[\"syntax.property\"]\ntext = \"blue\"\n[\"syntax.constructor\"]\ntext = \"lightcyan\"\n[\"syntax.variable.parameter\"]\ntext = \"gray\"\n";

fn typescript_colors(language: Language, source: &str) -> Vec<(String, Color)> {
    let theme = Theme::from_toml(TYPESCRIPT_THEME);
    let mut highlighter = Highlighter::new(language, &theme).unwrap();
    let rope = Rope::from_str(source);
    highlighter.parse(&rope).unwrap();
    highlighter
        .spans(&rope, 0..source.len())
        .into_iter()
        .filter_map(|span| Some((source[span.range].to_owned(), span.style.fg?)))
        .collect()
}

const TYPESCRIPT_SAMPLE: &str = "interface Shape { area(): number }\nenum Color { Red, Green = 2 }\n@sealed\nclass Box<T> implements Shape {\n  constructor(private readonly size: T) {}\n}\nfunction wrap<T>(value: T, extra?: string): T { return value as T; }\nconst n = box?.size;\nconst m = new Map<string, number>();\n";

#[test]
fn typescript_types_enums_decorators_and_parameters_get_their_own_colors() {
    let found = typescript_colors(Language::TypeScript, TYPESCRIPT_SAMPLE);

    // The names of types, and the types the grammar knows by themselves.
    for word in ["Shape", "Color", "Box", "T", "number", "string"] {
        assert!(has(&found, word, Color::Cyan), "{word}: {found:?}");
    }
    // What TypeScript adds to the keywords.
    for word in [
        "interface",
        "enum",
        "implements",
        "private",
        "readonly",
        "as",
    ] {
        assert!(has(&found, word, Color::Red), "{word}: {found:?}");
    }
    // The members of an enum.
    for word in ["Red", "Green"] {
        assert!(has(&found, word, Color::Magenta), "{word}: {found:?}");
    }
    assert!(
        has(&found, "@sealed", Color::Green),
        "a decorator: {found:?}"
    );
    // Parameters are not variables, though the JavaScript query takes every identifier for one.
    for word in ["size", "value", "extra"] {
        assert!(has(&found, word, Color::Gray), "{word}: {found:?}");
    }
    assert!(has(&found, "wrap", Color::Yellow), "a function: {found:?}");
    assert!(
        has(&found, "Map", Color::LightCyan),
        "a constructor: {found:?}"
    );
    assert!(
        has(&found, "size", Color::Blue),
        "a property after `?.`: {found:?}"
    );
    assert!(has(&found, "n", Color::White), "a variable: {found:?}");
}

#[test]
fn a_ts_file_that_holds_only_javascript_still_highlights_as_javascript_does() {
    let source = "function go() {}\nconst run = () => 1;\nlet x = new Map();\ngo();\nx.size();\nclass A { m() {} }\n";

    let found = typescript_colors(Language::TypeScript, source);

    for function in ["go", "run", "size", "m"] {
        assert!(
            has(&found, function, Color::Yellow),
            "{function}: {found:?}"
        );
    }
    assert!(
        has(&found, "Map", Color::LightCyan),
        "a constructor: {found:?}"
    );
    assert!(has(&found, "x", Color::White), "a variable: {found:?}");
}

#[test]
fn tsx_has_the_types_of_typescript_and_the_tags_of_jsx() {
    let source = "const a = <div><Foo bar={1} /></div>;\ninterface P { x: number }\nfunction f<T>(p: P) { return p as unknown as T; }\n";

    let found = typescript_colors(Language::Tsx, source);

    assert!(has(&found, "div", Color::Red), "a tag: {found:?}");
    assert!(
        has(&found, "Foo", Color::LightCyan),
        "a component: {found:?}"
    );
    assert!(has(&found, "interface", Color::Red), "{found:?}");
    assert!(has(&found, "P", Color::Cyan), "a type: {found:?}");
    assert!(has(&found, "p", Color::Gray), "a parameter: {found:?}");
}

#[test]
fn the_name_of_a_typescript_block_in_markdown_finds_the_language() {
    for name in ["ts", "typescript", "TypeScript", "mts", "cts"] {
        assert_eq!(
            Language::by_name(name),
            Some(Language::TypeScript),
            "{name}"
        );
    }
    for name in ["tsx", "TSX"] {
        assert_eq!(Language::by_name(name), Some(Language::Tsx), "{name}");
    }
}

#[test]
fn typescript_toggles_comments_with_slashes() {
    use crate::Comment;

    assert_eq!(Language::TypeScript.comment(), Some(Comment::Line("//")));
    assert_eq!(Language::Tsx.comment(), Some(Comment::Line("//")));
}
