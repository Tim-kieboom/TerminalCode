use ratatui::style::{Color, Modifier, Style};

use super::super::theme::*;

#[test]
fn known_slot_returns_its_style() {
    let theme = Theme::default();

    assert_eq!(
        theme.style("pane.border"),
        Style::default().fg(Color::DarkGray)
    );
}

#[test]
fn default_theme_styles_title_and_selection() {
    let theme = Theme::default();

    assert_eq!(
        theme.style("pane.title"),
        Style::default().add_modifier(Modifier::BOLD)
    );
    assert_eq!(
        theme.style("list.selected"),
        Style::default().add_modifier(Modifier::REVERSED)
    );
}

#[test]
fn unknown_slot_falls_back_to_default_style() {
    let theme = Theme::default();

    assert_eq!(theme.style("plugin.made.up"), Style::default());
}

#[test]
fn from_toml_reads_hex_colors_and_backgrounds() {
    let source = r##"["editor.bg"]
fg = "#102030"
bg = "red"
"##;

    let theme = Theme::from_toml(source).unwrap();

    assert_eq!(
        theme.style("editor.bg"),
        Style::default()
            .fg(Color::Rgb(0x10, 0x20, 0x30))
            .bg(Color::Red)
    );
}

#[test]
fn from_toml_rejects_unknown_color() {
    let source = r#"["pane.border"]
fg = "not_a_color"
"#;

    let result = Theme::from_toml(source);

    assert!(matches!(result, Err(ThemeError::InvalidColor { .. })));
}

#[test]
fn from_toml_rejects_unknown_modifier() {
    let source = r#"["pane.title"]
modifiers = ["sparkly"]
"#;

    let result = Theme::from_toml(source);

    assert!(matches!(result, Err(ThemeError::Parse(_))));
}

#[test]
fn from_toml_rejects_malformed_input() {
    let result = Theme::from_toml("[unterminated");

    assert!(matches!(result, Err(ThemeError::Parse(_))));
}
