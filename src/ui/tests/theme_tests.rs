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

#[test]
fn a_capture_uses_the_most_specific_syntax_slot_the_theme_has() {
    let theme = Theme::from_toml(
        "[\"syntax.function\"]\ntext = \"blue\"\n[\"syntax.function.macro\"]\ntext = \"red\"\n[\"syntax\"]\ntext = \"gray\"\n",
    )
    .unwrap();

    let fg = |capture| theme.syntax_style(capture).and_then(|style| style.fg);

    assert_eq!(fg("function.macro"), Some(Color::Red));
    assert_eq!(fg("function.builtin"), Some(Color::Blue));
    assert_eq!(fg("function"), Some(Color::Blue));
    assert_eq!(fg("keyword.control"), Some(Color::Gray), "plain `syntax`");
}

#[test]
fn a_capture_without_any_syntax_slot_has_no_style() {
    let theme = Theme::from_toml("[\"syntax.string\"]\ntext = \"green\"\n").unwrap();

    assert_eq!(theme.syntax_style("keyword"), None);
    assert_eq!(
        theme.syntax_style("string.escape").unwrap().fg,
        Some(Color::Green)
    );
}

// ---- palette

const PALETTE: &str = r##"
[palette]
accent = "#7f0e3f"
text = "#b9b9bd"
warning = "yellow"
"##;

fn themed(slots: &str) -> Result<Theme, ThemeError> {
    Theme::from_toml(&format!("{PALETTE}\n{slots}"))
}

#[test]
fn a_slot_color_can_name_a_palette_entry() {
    let theme = themed(
        r#"
        ["pane.border.focused"]
        text = "@accent"
        background = "@text"
        "#,
    )
    .unwrap();

    let style = theme.style("pane.border.focused");
    assert_eq!(style.fg, Some(Color::Rgb(0x7f, 0x0e, 0x3f)));
    assert_eq!(style.bg, Some(Color::Rgb(0xb9, 0xb9, 0xbd)));
}

#[test]
fn a_palette_entry_may_be_a_color_name() {
    let theme = themed("[\"search.match\"]\ntext = \"@warning\"\n").unwrap();

    assert_eq!(theme.style("search.match").fg, Some(Color::Yellow));
}

#[test]
fn a_literal_color_still_works_next_to_the_palette() {
    let theme = themed("[\"a\"]\ntext = \"@accent\"\n[\"b\"]\ntext = \"#112233\"\n").unwrap();

    assert_eq!(theme.style("a").fg, Some(Color::Rgb(0x7f, 0x0e, 0x3f)));
    assert_eq!(theme.style("b").fg, Some(Color::Rgb(0x11, 0x22, 0x33)));
}

#[test]
fn the_background_color_can_name_a_palette_entry() {
    let theme = themed("[background]\ncolor = \"@accent\"\n").unwrap();

    assert_eq!(theme.background_color(), Some(Color::Rgb(0x7f, 0x0e, 0x3f)));
}

#[test]
fn a_translucent_background_can_name_a_hex_palette_entry() {
    let theme = themed("[background]\ncolor = \"@accent\"\nopacity = 0.5\n").unwrap();

    assert!(theme.needs_terminal_background());
}

#[test]
fn an_unknown_palette_name_in_a_slot_names_the_slot_and_the_entry() {
    let error = themed("[\"pane.border\"]\ntext = \"@acent\"\n").unwrap_err();

    assert!(matches!(error, ThemeError::UnknownPaletteName { .. }));
    assert_eq!(
        error.to_string(),
        "slot `pane.border`: there is no palette color `acent`"
    );
}

#[test]
fn an_unknown_palette_name_in_the_background_is_an_error() {
    let error = themed("[background]\ncolor = \"@nope\"\n").unwrap_err();

    assert_eq!(
        error.to_string(),
        "background: there is no palette color `nope`"
    );
}

#[test]
fn a_name_is_an_error_without_a_palette_at_all() {
    let error = Theme::from_toml("[\"a\"]\ntext = \"@accent\"\n").unwrap_err();

    assert!(matches!(error, ThemeError::UnknownPaletteName { .. }));
}

#[test]
fn a_palette_entry_cannot_refer_to_another() {
    let error = Theme::from_toml("[palette]\na = \"#112233\"\nb = \"@a\"\n").unwrap_err();

    assert!(matches!(error, ThemeError::PaletteReference { ref name } if &**name == "b"));
}

#[test]
fn a_palette_entry_with_an_unknown_color_is_an_error() {
    let error = Theme::from_toml("[palette]\naccent = \"not-a-color\"\n").unwrap_err();

    assert_eq!(
        error.to_string(),
        "palette color `accent` has an unknown color `not-a-color`"
    );
}

#[test]
fn the_palette_table_is_not_a_style_slot() {
    let theme = themed("").unwrap();

    assert!(!theme.contains("palette"));
}

const DEFAULT_THEME_FILE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/defaults/default_theme.toml"
));

/// The slot colors of the built-in theme file: the value of every `text =` and
/// `background =` line after the `[palette]` table.
fn slot_colors_in_default_theme() -> Vec<&'static str> {
    let after_palette = DEFAULT_THEME_FILE
        .split_once("[palette]")
        .map(|(_, rest)| rest)
        .unwrap();
    let slots = after_palette
        .split_once("\n[\"")
        .map(|(_, rest)| rest)
        .unwrap();
    slots
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            matches!(key.trim(), "text" | "background").then(|| value.trim().trim_matches('"'))
        })
        .collect()
}

#[test]
fn the_default_theme_writes_its_colors_in_the_palette_not_in_the_slots() {
    let colors = slot_colors_in_default_theme();

    assert!(colors.len() > 40, "{}", colors.len());
    // `black` is the one literal: it is the text on a warning-colored match.
    let literals: Vec<_> = colors
        .iter()
        .filter(|color| !color.starts_with('@') && **color != "black")
        .collect();
    assert!(literals.is_empty(), "{literals:?}");
}

#[test]
fn every_palette_name_the_default_theme_uses_exists() {
    // Parsing fails on an unknown name, so this is the default theme parsing.
    assert!(Theme::from_toml(DEFAULT_THEME_FILE).is_ok());
}

#[test]
fn the_default_theme_palette_has_the_documented_roles() {
    let roles = [
        "text",
        "dim",
        "muted",
        "faint",
        "surface",
        "accent",
        "accent_bg",
        "accent_text",
        "on_accent",
        "selection",
        "info",
        "error",
        "warning",
        "red",
        "orange",
        "yellow",
        "green",
        "teal",
        "cyan",
        "blue",
        "purple",
        "pink",
        "comment",
        "punctuation",
    ];
    let palette = DEFAULT_THEME_FILE.split_once("[palette]").unwrap().1;

    for role in roles {
        assert!(
            palette
                .lines()
                .any(|line| line.trim_start().starts_with(role)
                    && line
                        .split_once('=')
                        .is_some_and(|(key, _)| key.trim() == role)),
            "{role}"
        );
    }
}

// ---- layering

const BASE: &str = r##"
[background]
color = "#101010"

[palette]
accent = "#7f0e3f"
text = "#b9b9bd"

["status.bar"]
text = "@text"
background = "#171725"
modifiers = ["bold"]

["pane.border"]
text = "@accent"

["tab.active"]
background = "@accent"
"##;

fn layered(user: &str) -> Theme {
    Theme::layered(BASE, user).unwrap()
}

#[test]
fn an_empty_user_file_leaves_the_base_as_it_is() {
    assert_eq!(layered(""), Theme::from_toml(BASE).unwrap());
}

#[test]
fn a_user_slot_changes_only_the_keys_it_names() {
    let theme = layered("[\"status.bar\"]\ntext = \"#ff0000\"\n");

    let style = theme.style("status.bar");
    assert_eq!(style.fg, Some(Color::Rgb(255, 0, 0)));
    assert_eq!(style.bg, Some(Color::Rgb(0x17, 0x17, 0x25)), "kept");
    assert!(style.add_modifier.contains(Modifier::BOLD), "kept");
}

#[test]
fn modifiers_replace_the_whole_list_and_an_empty_list_clears_it() {
    let italic = layered("[\"status.bar\"]\nmodifiers = [\"italic\"]\n");
    let cleared = layered("[\"status.bar\"]\nmodifiers = []\n");

    let style = italic.style("status.bar");
    assert!(style.add_modifier.contains(Modifier::ITALIC));
    assert!(!style.add_modifier.contains(Modifier::BOLD));
    assert_eq!(cleared.style("status.bar").add_modifier, Modifier::empty());
}

#[test]
fn reset_removes_a_color_the_base_sets() {
    let theme = layered("[\"status.bar\"]\nbackground = \"reset\"\n");

    let style = theme.style("status.bar");
    assert_eq!(style.bg, None);
    assert_eq!(
        style.fg,
        Some(Color::Rgb(0xb9, 0xb9, 0xbd)),
        "the text stays"
    );
}

#[test]
fn a_slot_the_user_does_not_mention_stays_as_the_base_has_it() {
    let theme = layered("[\"status.bar\"]\ntext = \"red\"\n");

    assert_eq!(
        theme.style("pane.border"),
        Theme::from_toml(BASE).unwrap().style("pane.border")
    );
}

#[test]
fn a_user_slot_the_base_lacks_is_added() {
    let theme = layered("[\"my.plugin\"]\ntext = \"green\"\n");

    assert_eq!(theme.style("my.plugin").fg, Some(Color::Green));
    assert!(theme.contains("pane.border"));
}

#[test]
fn redefining_a_palette_entry_restyles_every_slot_that_uses_it() {
    let theme = layered("[palette]\naccent = \"#00ff00\"\n");

    let green = Some(Color::Rgb(0, 255, 0));
    assert_eq!(theme.style("pane.border").fg, green);
    assert_eq!(theme.style("tab.active").bg, green);
    assert_eq!(
        theme.style("status.bar").fg,
        Some(Color::Rgb(0xb9, 0xb9, 0xbd)),
        "other entries are untouched"
    );
}

#[test]
fn a_slot_in_the_user_file_can_name_a_base_palette_entry() {
    let theme = layered("[\"my.plugin\"]\ntext = \"@accent\"\n");

    assert_eq!(
        theme.style("my.plugin").fg,
        Some(Color::Rgb(0x7f, 0x0e, 0x3f))
    );
}

#[test]
fn a_literal_in_a_user_slot_beats_a_redefined_palette_entry() {
    let theme = layered("[palette]\naccent = \"#00ff00\"\n[\"pane.border\"]\ntext = \"#0000ff\"\n");

    assert_eq!(theme.style("pane.border").fg, Some(Color::Rgb(0, 0, 255)));
    assert_eq!(theme.style("tab.active").bg, Some(Color::Rgb(0, 255, 0)));
}

#[test]
fn the_background_is_merged_key_by_key() {
    let theme = layered("[background]\nopacity = 1.0\n");
    let base = Theme::from_toml(BASE).unwrap();

    assert_eq!(theme.background_color(), base.background_color());

    let recolored = layered("[background]\ncolor = \"@accent\"\n");
    assert_eq!(
        recolored.background_color(),
        Some(Color::Rgb(0x7f, 0x0e, 0x3f))
    );
}

#[test]
fn an_unknown_palette_name_in_the_user_file_is_an_error() {
    let error = Theme::layered(BASE, "[\"status.bar\"]\ntext = \"@nope\"\n").unwrap_err();

    assert!(matches!(error, ThemeError::UnknownPaletteName { .. }));
}

#[test]
fn an_invalid_user_file_is_an_error_and_a_syntax_error_too() {
    assert!(Theme::layered(BASE, "[\"a\"]\ntext = \"not-a-color\"\n").is_err());
    assert!(matches!(
        Theme::layered(BASE, "[[["),
        Err(ThemeError::Parse(_))
    ));
}

#[test]
fn layering_the_built_in_theme_with_nothing_is_the_built_in_theme() {
    let layered = Theme::layered(DEFAULT_THEME_FILE, "").unwrap();

    assert_eq!(layered, Theme::default());
}

#[test]
fn a_user_can_retint_the_built_in_theme_with_one_palette_entry() {
    let layered = Theme::layered(DEFAULT_THEME_FILE, "[palette]\norange = \"#00ff00\"\n").unwrap();

    let green = Some(Color::Rgb(0, 255, 0));
    assert_eq!(layered.syntax_style("string").unwrap().fg, green);
    assert_ne!(
        layered.syntax_style("function").unwrap().fg,
        green,
        "only what uses orange changes"
    );
}
