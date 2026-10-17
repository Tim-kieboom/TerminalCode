use super::*;

#[test]
fn switching_themes_works_with_an_overlay_in_the_config_folder() {
    let dir = tempfile::tempdir().unwrap();
    install_built_in_copies(dir.path());
    write(dir.path(), "theme.toml", "# nothing in it\n");
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: light");

    assert_eq!(*app.state().theme(), Theme::built_in(ThemeVariant::Light));
}

#[test]
fn an_overlay_that_is_a_copy_of_the_dark_theme_does_not_pin_it() {
    // What older versions of Open Theme wrote.
    let dir = tempfile::tempdir().unwrap();
    install_built_in_copies(dir.path());
    write(dir.path(), "theme.toml", DEFAULT_THEME_TOML);
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: light");

    assert_eq!(*app.state().theme(), Theme::built_in(ThemeVariant::Light));
}

#[test]
fn a_copy_of_the_dark_theme_does_not_hide_an_installed_theme_either() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    write(dir.path(), "theme.toml", DEFAULT_THEME_TOML);
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: solar");

    assert_eq!(accent(&app), Some(Color::Rgb(0, 255, 0)));
}

#[test]
fn an_overlay_that_changes_something_is_still_applied_over_the_light_theme() {
    let dir = tempfile::tempdir().unwrap();
    install_built_in_copies(dir.path());
    write(
        dir.path(),
        "theme.toml",
        &format!("{DEFAULT_THEME_TOML}\n# edited\n[\"my.slot\"]\ntext = \"red\"\n"),
    );
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: light");

    // A file that differs from the built-in one is the user's, and applies in full.
    assert_eq!(app.state().theme().style("my.slot").fg, Some(Color::Red));
    assert_ne!(*app.state().theme(), Theme::built_in(ThemeVariant::Light));
}
