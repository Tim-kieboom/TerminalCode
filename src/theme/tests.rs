use ratatui::style::Color;
use serde_json::{Value, json};

use super::{ThemeConfig, parse_color};

fn parse_str(s: &str) -> Color {
    parse_color(&Value::String(s.to_string())).unwrap()
}

#[test]
fn parses_six_digit_hex() {
    assert_eq!(parse_str("#80c8ff"), Color::Rgb(0x80, 0xc8, 0xff));
}

#[test]
fn parses_hex_without_prefix() {
    assert_eq!(parse_str("80c8ff"), Color::Rgb(0x80, 0xc8, 0xff));
}

#[test]
fn parses_three_digit_hex() {
    assert_eq!(parse_str("#abc"), Color::Rgb(0xaa, 0xbb, 0xcc));
}

#[test]
fn parses_rgb_array() {
    assert_eq!(parse_color(&json!([1, 2, 3])), Some(Color::Rgb(1, 2, 3)));
}

#[test]
fn parses_named_colors() {
    assert_eq!(parse_str("red"), Color::Red);
    assert_eq!(parse_str("yellow"), Color::Yellow);
    assert_eq!(parse_str("gray"), Color::Gray);
    assert_eq!(parse_str("grey"), Color::Gray);
    assert_eq!(parse_str("reset"), Color::Reset);
}

#[test]
fn invalid_inputs_return_none() {
    assert_eq!(parse_color(&Value::Null), None);
    assert_eq!(parse_color(&json!([1, 2, 3, 4])), None);
    assert_eq!(parse_color(&json!([1, 2])), None);
    assert_eq!(parse_color(&Value::String("notacolor".into())), None);
    assert_eq!(parse_color(&Value::String("#12345".into())), None);
}

#[test]
fn load_merges_over_hardcoded_defaults() {
    let config = ThemeConfig::load(r##"{ "accent": "#ff0000" }"##);
    assert_eq!(config.accent, Color::Rgb(255, 0, 0));
    assert_eq!(config.text, Color::Rgb(220, 220, 220));
}

#[test]
fn load_ignores_unknown_keys_and_bad_values() {
    let config = ThemeConfig::load(r##"{ "bogus": "#ff0000", "accent": "notacolor" }"##);
    assert_eq!(config.accent, Color::Rgb(80, 200, 255));
}

#[test]
fn load_falls_back_to_defaults_on_invalid_json() {
    let config = ThemeConfig::load("not json");
    assert_eq!(config.accent, Color::Rgb(80, 200, 255));
}
