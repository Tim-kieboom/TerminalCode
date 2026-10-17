use super::*;

#[test]
fn the_sol_themes_are_built_in_templates() {
    for variant in [ThemeVariant::SolDark, ThemeVariant::SolLight] {
        assert!(Theme::from_toml(variant.source()).is_ok(), "{variant:?}");
        assert!(crate::config::theme_templates(None).contains(&variant.title().to_owned()));
    }
}

#[test]
fn choosing_a_sol_theme_applies_its_interface_colors() {
    for (query, variant) in [
        ("Theme: soldark", ThemeVariant::SolDark),
        ("Theme: sollight", ThemeVariant::SolLight),
    ] {
        let (mut app, _dir) = app_with_built_in_copies();

        run_from_palette(&mut app, query);

        assert_eq!(*app.state().theme(), Theme::built_in(variant), "{query}");
    }
}

#[test]
fn soldark_has_the_given_colors_and_the_default_syntax_colors() {
    let sol = Theme::built_in(ThemeVariant::SolDark);
    let dark = Theme::default();

    assert_eq!(
        sol.style("pane.border.focused").fg,
        Some(Color::Rgb(0xd1, 0xc8, 0x8f))
    );
    assert_eq!(
        sol.style("status.bar").bg,
        Some(Color::Rgb(0x1e, 0x1e, 0x2e))
    );
    assert_eq!(
        sol.style("explorer.selected").bg,
        Some(Color::Rgb(0x28, 0x2e, 0x1e))
    );
    for capture in [
        "keyword", "string", "function", "type", "number", "comment", "variable", "operator",
    ] {
        assert_eq!(
            sol.syntax_style(capture),
            dark.syntax_style(capture),
            "{capture}"
        );
    }
}

#[test]
fn sollight_has_the_light_syntax_colors() {
    let sol = Theme::built_in(ThemeVariant::SolLight);
    let light = Theme::built_in(ThemeVariant::Light);

    for capture in ["keyword", "string", "function", "type", "number", "comment"] {
        assert_eq!(
            sol.syntax_style(capture),
            light.syntax_style(capture),
            "{capture}"
        );
    }
    assert_ne!(sol.style("status.bar"), light.style("status.bar"));
}

#[test]
fn the_sol_themes_have_the_slots_of_the_dark_theme() {
    let dark = DEFAULT_THEME_TOML.parse::<toml::Table>().unwrap();
    for variant in [ThemeVariant::SolDark, ThemeVariant::SolLight] {
        let sol = variant.source().parse::<toml::Table>().unwrap();
        let names =
            |t: &toml::Table| -> std::collections::BTreeSet<String> { t.keys().cloned().collect() };
        assert_eq!(names(&sol), names(&dark), "{variant:?}");
    }
}

#[test]
fn a_new_theme_can_start_from_a_sol_theme() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());

    new_theme(&mut app, "SolDark", "mine");

    assert_eq!(
        fs::read_to_string(dir.path().join("themes/mine.toml")).unwrap(),
        crate::ui::theme::SOL_DARK_THEME_TOML
    );
    assert_eq!(*app.state().theme(), Theme::built_in(ThemeVariant::SolDark));
}

#[test]
fn a_theme_file_that_is_an_unchanged_copy_of_a_sol_theme_does_not_pin_it() {
    let dir = tempfile::tempdir().unwrap();
    install_built_in_copies(dir.path());
    write(
        dir.path(),
        "theme.toml",
        crate::ui::theme::SOL_DARK_THEME_TOML,
    );
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: light");

    assert_eq!(*app.state().theme(), Theme::built_in(ThemeVariant::Light));
}
