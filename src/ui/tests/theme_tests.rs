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
    let source = r##"["editor.sample"]
text = "#102030"
background = "red"
"##;

    let theme = Theme::from_toml(source).unwrap();

    assert_eq!(
        theme.style("editor.sample"),
        Style::default()
            .fg(Color::Rgb(0x10, 0x20, 0x30))
            .bg(Color::Red)
    );
}

#[test]
fn from_toml_rejects_unknown_color() {
    let source = r#"["pane.border"]
text = "not_a_color"
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

// ---- [background]

fn background(toml: &str) -> Result<Theme, ThemeError> {
    Theme::from_toml(toml)
}

fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb { r, g, b }
}

#[test]
fn a_theme_without_a_background_leaves_the_terminals_alone() {
    let theme = Theme::from_toml("").unwrap();

    assert_eq!(theme.background_color(), None);
    assert!(!theme.needs_terminal_background());
}

#[test]
fn a_solid_background_is_painted_as_written() {
    let theme = background("[background]\ncolor = \"#102030\"\n").unwrap();

    assert_eq!(theme.background_color(), Some(Color::Rgb(0x10, 0x20, 0x30)));
    assert!(!theme.needs_terminal_background());
}

#[test]
fn a_named_color_works_when_fully_opaque() {
    let theme = background("[background]\ncolor = \"blue\"\nopacity = 1.0\n").unwrap();

    assert_eq!(theme.background_color(), Some(Color::Blue));
}

#[test]
fn transparent_means_nothing_is_painted() {
    for source in [
        "[background]\ncolor = \"transparent\"\n",
        "[background]\ncolor = \"Reset\"\n",
        "[background]\ncolor = \"#102030\"\nopacity = 0\n",
        "[background]\ncolor = \"#102030\"\nopacity = 0.0\n",
        "[background]\n",
    ] {
        let theme = background(source).unwrap();

        assert_eq!(theme.background_color(), None, "{source}");
        assert!(!theme.needs_terminal_background(), "{source}");
    }
}

#[test]
fn an_opacity_below_one_blends_with_the_terminals_background() {
    let mut theme = background("[background]\ncolor = \"#f0f0f0\"\nopacity = 0.5\n").unwrap();
    assert!(theme.needs_terminal_background());

    theme.set_terminal_background(Some(rgb(0, 0, 0)));

    // Half way between 0xf0 and 0: 120 (alpha 128 of 255 rounds to 0x78).
    assert_eq!(theme.background_color(), Some(Color::Rgb(0x78, 0x78, 0x78)));
}

#[test]
fn the_blend_moves_toward_the_terminal_as_opacity_drops() {
    let channel = |opacity: &str| {
        let source = format!("[background]\ncolor = \"#ffffff\"\nopacity = {opacity}\n");
        let mut theme = background(&source).unwrap();
        theme.set_terminal_background(Some(rgb(0, 0, 0)));
        match theme.background_color() {
            Some(Color::Rgb(r, _, _)) => r,
            other => panic!("{other:?}"),
        }
    };

    let (high, mid, low) = (channel("0.9"), channel("0.5"), channel("0.1"));

    assert!(high > mid && mid > low, "{high} {mid} {low}");
    assert!(low > 0 && high < 255);
}

#[test]
fn the_blend_works_per_channel_over_a_colored_terminal() {
    let mut theme = background("[background]\ncolor = \"#ff0000\"\nopacity = 0.5\n").unwrap();
    theme.set_terminal_background(Some(rgb(0, 0, 255)));

    assert_eq!(theme.background_color(), Some(Color::Rgb(128, 0, 127)));
}

#[test]
fn without_knowing_the_terminals_background_the_tint_color_is_used_as_is() {
    let theme = background("[background]\ncolor = \"#336699\"\nopacity = 0.3\n").unwrap();

    assert_eq!(theme.background_color(), Some(Color::Rgb(0x33, 0x66, 0x99)));
}

#[test]
fn the_blend_endpoints_are_the_two_colors() {
    let front = rgb(200, 100, 50);
    let behind = rgb(10, 20, 30);

    assert_eq!(front.over(behind, 255), front);
    assert_eq!(front.over(behind, 0), behind);
}

#[test]
fn an_integer_opacity_is_accepted() {
    assert!(background("[background]\ncolor = \"#102030\"\nopacity = 1\n").is_ok());
}

#[test]
fn an_opacity_outside_zero_to_one_is_rejected() {
    for value in ["1.5", "-0.1", "2", "-1"] {
        let source = format!("[background]\ncolor = \"#102030\"\nopacity = {value}\n");

        assert!(
            matches!(background(&source), Err(ThemeError::InvalidOpacity(_))),
            "{value}"
        );
    }
}

#[test]
fn an_opacity_without_a_color_is_rejected() {
    assert!(matches!(
        background("[background]\nopacity = 0.5\n"),
        Err(ThemeError::OpacityWithoutColor)
    ));
}

#[test]
fn a_translucent_named_color_is_rejected_because_its_rgb_is_unknown() {
    let result = background("[background]\ncolor = \"blue\"\nopacity = 0.5\n");

    assert!(matches!(result, Err(ThemeError::OpacityNeedsHex(color)) if &*color == "blue"));
}

#[test]
fn an_unknown_background_color_is_named_in_the_error() {
    let error = background("[background]\ncolor = \"not_a_color\"\n").unwrap_err();

    assert!(matches!(error, ThemeError::InvalidBackgroundColor(_)));
    assert!(error.to_string().contains("not_a_color"));
}

#[test]
fn a_misspelled_background_key_is_an_error() {
    assert!(matches!(
        background("[background]\ncolour = \"#102030\"\n"),
        Err(ThemeError::Parse(_))
    ));
}

#[test]
fn the_background_table_does_not_disturb_the_style_slots() {
    let theme =
        background("[background]\ncolor = \"#102030\"\n\n[\"pane.border\"]\nfg = \"red\"\n")
            .unwrap();

    assert_eq!(theme.style("pane.border"), Style::default().fg(Color::Red));
    assert!(!theme.contains("background"));
}

// ---- blend = "none" (let the terminal make the exact color translucent)

#[test]
fn blend_none_paints_the_exact_color_even_with_opacity() {
    let mut theme =
        background("[background]\ncolor = \"#1e1e2e\"\nopacity = 0.3\nblend = \"none\"\n").unwrap();
    theme.set_terminal_background(Some(rgb(255, 255, 255)));

    assert_eq!(theme.background_color(), Some(Color::Rgb(0x1e, 0x1e, 0x2e)));
    assert!(!theme.needs_terminal_background());
}

#[test]
fn blend_terminal_is_the_default_and_can_be_spelled_out() {
    let implicit = background("[background]\ncolor = \"#1e1e2e\"\nopacity = 0.3\n").unwrap();
    let explicit =
        background("[background]\ncolor = \"#1e1e2e\"\nopacity = 0.3\nblend = \"terminal\"\n")
            .unwrap();

    assert_eq!(implicit, explicit);
    assert!(implicit.needs_terminal_background());
}

#[test]
fn blend_none_names_the_kitty_setting_in_a_hint() {
    let theme =
        background("[background]\ncolor = \"#1e1e2e\"\nopacity = 0.3\nblend = \"none\"\n").unwrap();

    let hint = theme.terminal_hint().unwrap();

    assert!(
        hint.contains("transparent_background_colors #1e1e2e@0.30"),
        "{hint}"
    );
    assert!(hint.contains("kitty.conf"), "{hint}");
}

#[test]
fn only_blend_none_with_an_opacity_below_one_gives_a_hint() {
    for source in [
        "",
        "[background]\ncolor = \"#1e1e2e\"\n",
        "[background]\ncolor = \"#1e1e2e\"\nopacity = 0.3\n",
        "[background]\ncolor = \"transparent\"\nblend = \"none\"\n",
        "[background]\ncolor = \"#1e1e2e\"\nopacity = 1\nblend = \"none\"\n",
    ] {
        assert_eq!(
            background(source).unwrap().terminal_hint(),
            None,
            "{source}"
        );
    }
}

#[test]
fn blend_none_still_needs_a_hex_color_below_full_opacity() {
    let result = background("[background]\ncolor = \"blue\"\nopacity = 0.5\nblend = \"none\"\n");

    assert!(matches!(result, Err(ThemeError::OpacityNeedsHex(_))));
}

#[test]
fn an_unknown_blend_mode_is_an_error() {
    let result = background("[background]\ncolor = \"#102030\"\nblend = \"frosted\"\n");

    assert!(matches!(result, Err(ThemeError::Parse(_))));
}

#[test]
fn the_old_fg_and_bg_names_still_work() {
    let source = r##"["editor.sample"]
fg = "#102030"
bg = "red"
"##;

    let theme = Theme::from_toml(source).unwrap();

    assert_eq!(
        theme.style("editor.sample"),
        Style::default()
            .fg(Color::Rgb(0x10, 0x20, 0x30))
            .bg(Color::Red)
    );
}

#[test]
fn a_misspelled_slot_key_is_an_error_not_silently_ignored() {
    let source = r#"["pane.border"]
txet = "red"
"#;

    assert!(Theme::from_toml(source).is_err());
}

#[test]
fn highlighted_slots_set_their_own_colors_instead_of_reversing_the_ones_below() {
    let theme = Theme::default();

    for slot in ["tab.active", "status.bar", "explorer.selected"] {
        let style = theme.style(slot);
        assert!(style.bg.is_some(), "{slot} has a background");
        assert!(
            !style
                .add_modifier
                .contains(ratatui::style::Modifier::REVERSED),
            "{slot} must not be reversed"
        );
    }
}
