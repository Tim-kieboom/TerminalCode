use super::*;

#[test]
fn the_palette_lists_no_built_in_theme_to_choose_only_templates() {
    let actions = Action::palette_actions();
    assert!(actions.contains(&Action::ChooseTheme));
    assert!(actions.contains(&Action::NewTheme));
    assert!(actions.iter().filter_map(Action::title).all(|title| {
        ![
            "Theme: Dark",
            "Theme: Light",
            "Theme: SolDark",
            "Theme: SolLight",
        ]
        .contains(&title)
    }));
}

#[test]
fn choosing_the_light_theme_restyles_the_editor() {
    let (mut app, _dir) = app_with_built_in_copies();
    let before = app.state().theme().syntax_style("keyword");

    use_theme(&mut app, "Theme: light");

    let after = app.state().theme().syntax_style("keyword");
    assert_ne!(after, before);
    assert_eq!(
        after,
        Theme::built_in(ThemeVariant::Light).syntax_style("keyword")
    );
}

#[test]
fn choosing_the_dark_theme_goes_back() {
    let (mut app, _dir) = app_with_built_in_copies();
    use_theme(&mut app, "Theme: light");

    use_theme(&mut app, "Theme: dark");

    assert_eq!(*app.state().theme(), Theme::default());
}

#[test]
fn a_theme_change_says_which_theme_it_is() {
    let (mut app, _dir) = app_with_built_in_copies();
    use_theme(&mut app, "Theme: light");
    assert!(
        app.state()
            .notifications()
            .texts()
            .iter()
            .any(|text| text.starts_with("theme: light"))
    );
}

#[test]
fn the_user_theme_file_is_laid_over_the_chosen_theme() {
    let dir = tempfile::tempdir().unwrap();
    install_built_in_copies(dir.path());
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );
    let mut app = app_with_config_dir(dir.path());

    use_theme(&mut app, "Theme: light");

    let focused = app.state().theme().style("pane.border.focused");
    assert_eq!(focused.fg, Some(Color::Rgb(0, 255, 0)));
    // Everything the file leaves alone is the light theme's.
    assert_eq!(
        app.state().theme().style("status.bar"),
        Theme::built_in(ThemeVariant::Light).style("status.bar")
    );
}

#[test]
fn a_broken_user_theme_leaves_the_chosen_built_in_theme_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    install_built_in_copies(dir.path());
    write(dir.path(), "theme.toml", "[palette\n");
    let mut app = app_with_config_dir(dir.path());

    use_theme(&mut app, "Theme: light");

    assert_eq!(
        app.state().theme().style("status.bar"),
        Theme::built_in(ThemeVariant::Light).style("status.bar")
    );
    assert!(app.state().notifications().has_errors());
}

#[test]
fn reload_keeps_the_chosen_theme() {
    let (mut app, _dir) = app_with_built_in_copies();
    use_theme(&mut app, "Theme: light");

    reload(&mut app);

    assert_eq!(
        app.state().theme().style("status.bar"),
        Theme::built_in(ThemeVariant::Light).style("status.bar")
    );
}

#[test]
fn a_theme_change_keeps_the_terminal_background() {
    let (mut app, _dir) = app_with_built_in_copies();
    let mut theme = Theme::default();
    theme.set_terminal_background(Some(Rgb { r: 1, g: 2, b: 3 }));
    app.state_mut().set_theme(theme);

    use_theme(&mut app, "Theme: light");

    assert_eq!(
        app.state().theme().terminal_background(),
        Some(Rgb { r: 1, g: 2, b: 3 })
    );
}
