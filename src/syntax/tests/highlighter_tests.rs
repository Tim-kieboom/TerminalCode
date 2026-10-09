use ratatui::style::Color;

use crate::syntax::{Highlighter, Language, Span};
use crate::ui::theme::Theme;

fn theme(source: &str) -> Theme {
    Theme::from_toml(source).unwrap()
}

fn highlighter(theme: &Theme) -> Highlighter {
    Highlighter::new(Language::Rust, theme).unwrap()
}

/// The text of each span with its color, for readable assertions.
fn painted(text: &str, spans: &[Span]) -> Vec<(String, Option<Color>)> {
    spans
        .iter()
        .map(|span| (text[span.range.clone()].to_owned(), span.style.fg))
        .collect()
}

fn parsed(source: &str, theme: &Theme) -> (Highlighter, String) {
    let mut highlighter = highlighter(theme);
    highlighter.parse(source).unwrap();
    (highlighter, source.to_owned())
}

const KEYWORD_AND_STRING: &str = r##"
["syntax.keyword"]
text = "red"
["syntax.string"]
text = "green"
"##;

#[test]
fn keywords_and_strings_get_the_style_of_their_capture() {
    let theme = theme(KEYWORD_AND_STRING);
    let (highlighter, text) = parsed("fn main() { let s = \"hi\"; }", &theme);

    let spans = highlighter.spans(&text, 0..text.len());

    assert_eq!(
        painted(&text, &spans),
        [
            ("fn".to_owned(), Some(Color::Red)),
            ("let".to_owned(), Some(Color::Red)),
            ("\"hi\"".to_owned(), Some(Color::Green)),
        ]
    );
}

#[test]
fn spans_are_in_order_and_never_overlap() {
    let (highlighter, text) = parsed(
        "// c\nuse std::fmt;\nfn f<T: Clone>(x: &T) -> Option<T> { Some(x.clone()) }\n",
        &Theme::default(),
    );

    let spans = highlighter.spans(&text, 0..text.len());

    assert!(!spans.is_empty());
    for pair in spans.windows(2) {
        assert!(pair[0].range.end <= pair[1].range.start, "{pair:?}");
    }
    for span in &spans {
        assert!(span.range.start < span.range.end);
        assert!(text.is_char_boundary(span.range.start) && text.is_char_boundary(span.range.end));
    }
}

#[test]
fn a_capture_the_theme_does_not_know_leaves_the_text_plain() {
    let theme = theme("[\"syntax.string\"]\ntext = \"green\"\n");
    let (highlighter, text) = parsed("fn main() { \"hi\" }", &theme);

    let spans = highlighter.spans(&text, 0..text.len());

    assert_eq!(
        painted(&text, &spans),
        [("\"hi\"".to_owned(), Some(Color::Green))]
    );
}

#[test]
fn a_capture_falls_back_to_the_shorter_slot() {
    let theme = theme("[\"syntax.function\"]\ntext = \"blue\"\n");
    let (highlighter, text) = parsed("fn main() { println!(\"x\"); }", &theme);

    let spans = highlighter.spans(&text, 0..text.len());

    // `main` is a function, `println!` a function.macro: both use syntax.function.
    let names: Vec<_> = painted(&text, &spans).into_iter().map(|(t, _)| t).collect();
    assert!(names.contains(&"main".to_owned()), "{names:?}");
    assert!(names.contains(&"println!".to_owned()), "{names:?}");
}

#[test]
fn a_more_specific_slot_beats_the_shorter_one() {
    let theme = theme(
        "[\"syntax.function\"]\ntext = \"blue\"\n[\"syntax.function.macro\"]\ntext = \"magenta\"\n",
    );
    let (highlighter, text) = parsed("fn main() { println!(\"x\"); }", &theme);

    let spans = highlighter.spans(&text, 0..text.len());

    let colors = painted(&text, &spans);
    assert!(colors.contains(&("main".to_owned(), Some(Color::Blue))));
    assert!(colors.contains(&("println!".to_owned(), Some(Color::Magenta))));
}

#[test]
fn only_the_asked_range_is_returned_and_edges_are_cut() {
    let theme = theme(KEYWORD_AND_STRING);
    let (highlighter, text) = parsed("fn a() {}\nfn b() {}\nfn c() {}\n", &theme);
    let start = text.find("fn b").unwrap();
    let end = start + "fn b".len();

    let spans = highlighter.spans(&text, start..end);

    assert_eq!(
        painted(&text, &spans),
        [("fn".to_owned(), Some(Color::Red))]
    );
    assert!(
        spans
            .iter()
            .all(|s| s.range.start >= start && s.range.end <= end)
    );

    // A range that cuts through a string keeps the part inside it.
    let text = "let s = \"abcdef\";\n";
    let (highlighter, text) = parsed(text, &theme);
    let open = text.find("abc").unwrap();
    let spans = highlighter.spans(&text, open..open + 3);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].range, open..open + 3);
}

#[test]
fn nothing_is_returned_before_the_text_is_parsed() {
    let highlighter = highlighter(&Theme::default());

    assert!(highlighter.spans("fn main() {}", 0..12).is_empty());
}

#[test]
fn parsing_again_replaces_the_old_tree() {
    let theme = theme(KEYWORD_AND_STRING);
    let mut highlighter = highlighter(&theme);
    highlighter.parse("fn a() {}").unwrap();

    highlighter.parse("\"only a string\"").unwrap();

    let text = "\"only a string\"";
    let spans = highlighter.spans(text, 0..text.len());
    assert_eq!(
        painted(text, &spans),
        [(text.to_owned(), Some(Color::Green))]
    );
}

#[test]
fn text_with_syntax_errors_still_gets_the_colors_it_can() {
    let theme = theme(KEYWORD_AND_STRING);
    let (highlighter, text) = parsed("fn ( { let \"unclosed", &theme);

    let spans = highlighter.spans(&text, 0..text.len());

    assert!(spans.iter().any(|s| s.style.fg == Some(Color::Red)));
}

#[test]
fn a_range_past_the_end_of_the_text_is_clamped() {
    let theme = theme(KEYWORD_AND_STRING);
    let (highlighter, text) = parsed("fn a() {}", &theme);

    let spans = highlighter.spans(&text, 0..10_000);

    assert_eq!(painted(&text, &spans).len(), 1);
}

#[test]
fn multibyte_text_gets_byte_offsets_on_char_boundaries() {
    let theme = theme(KEYWORD_AND_STRING);
    let (highlighter, text) = parsed("let s = \"héllo wörld ✓\"; let t = 1;", &theme);

    let spans = highlighter.spans(&text, 0..text.len());

    assert!(painted(&text, &spans).contains(&("\"héllo wörld ✓\"".to_owned(), Some(Color::Green))));
}

#[test]
fn the_default_theme_colors_ordinary_rust() {
    let (highlighter, text) = parsed(
        "/// docs\n#[derive(Debug)]\nstruct P { x: u32 }\nfn main() { let p = P { x: 1 }; println!(\"{}\", p.x); }\n",
        &Theme::default(),
    );

    let spans = highlighter.spans(&text, 0..text.len());

    let colored: Vec<_> = painted(&text, &spans).into_iter().map(|(t, _)| t).collect();
    for expected in ["struct", "fn", "let", "u32", "main", "println!"] {
        assert!(
            colored.iter().any(|t| t == expected),
            "{expected}: {colored:?}"
        );
    }
}
