use super::*;

#[test]
fn open_theme_opens_the_installed_theme_that_was_chosen() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());
    run_from_palette(&mut app, "Theme: solar");

    run_from_palette(&mut app, "Open Theme");

    assert_eq!(editor_text(&app), GREEN_ACCENT);
    assert!(
        !dir.path().join("theme.toml").exists(),
        "no overlay was made"
    );
}

#[test]
fn an_installed_theme_whose_file_vanished_is_reported_and_nothing_is_made() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());
    run_from_palette(&mut app, "Theme: solar");
    fs::remove_file(dir.path().join("themes/solar.toml")).unwrap();

    run_from_palette(&mut app, "Open Theme");

    assert!(app.state().notifications().has_errors());
    assert!(!dir.path().join("themes/solar.toml").exists());
}
