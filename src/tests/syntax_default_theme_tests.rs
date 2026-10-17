//! The theme that ships with the editor colors ordinary Rust.

use ropey::Rope;

use crate::syntax::Highlighter;
use crate::syntax::Language;
use crate::ui::theme::Theme;

#[test]
fn the_default_theme_colors_ordinary_rust() {
    let text = "/// docs\n#[derive(Debug)]\nstruct P { x: u32 }\nfn main() { let p = P { x: 1 }; println!(\"{}\", p.x); }\n".to_owned();
    let mut highlighter = Highlighter::new(Language::Rust, &Theme::default()).unwrap();
    highlighter.parse(&Rope::from_str(&text)).unwrap();

    let spans = highlighter.spans(&Rope::from_str(&text), 0..text.len());

    let colored: Vec<String> = spans
        .iter()
        .map(|span| text[span.range.clone()].to_owned())
        .collect();
    for expected in ["struct", "fn", "let", "u32", "main", "println!"] {
        assert!(
            colored.iter().any(|t| t == expected),
            "{expected}: {colored:?}"
        );
    }
}
