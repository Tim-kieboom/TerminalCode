use super::*;

#[test]
fn installed_themes_are_listed_by_name_without_the_extension() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    install(dir.path(), "mono", GREEN_ACCENT);
    fs::write(dir.path().join("themes/readme.txt"), "x").unwrap();
    fs::create_dir(dir.path().join("themes/folder.toml")).unwrap();

    assert_eq!(
        crate::config::installed_themes(dir.path()),
        ["mono", "solar"]
    );
}

#[test]
fn no_themes_folder_means_no_installed_themes() {
    let dir = tempfile::tempdir().unwrap();
    assert!(crate::config::installed_themes(dir.path()).is_empty());
    assert!(crate::config::installed_themes(&dir.path().join("missing")).is_empty());
}

#[test]
fn a_theme_name_cannot_reach_outside_the_themes_folder() {
    let dir = Path::new("/cfg");
    assert!(crate::config::installed_theme_path(dir, "../keymap").is_none());
    assert!(crate::config::installed_theme_path(dir, "a/b").is_none());
    assert!(crate::config::installed_theme_path(dir, "..").is_none());
    assert!(crate::config::installed_theme_path(dir, "").is_none());
    assert_eq!(
        crate::config::installed_theme_path(dir, "solar").unwrap(),
        Path::new("/cfg/themes/solar.toml")
    );
}

#[test]
fn the_palette_lists_each_installed_theme() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());

    press(&mut app, KeyCode::F(1));
    type_str(&mut app, "Theme: solar");

    let titles: Vec<_> = app
        .state()
        .palette()
        .unwrap()
        .visible()
        .map(|entry| entry.title.clone())
        .collect();
    // The best matches; looser ones (the letters of "solar" are also in "SolDark") follow.
    assert_eq!(titles[..2], ["Theme: solar", "Theme: New from solar"]);
}

#[test]
fn choosing_an_installed_theme_applies_it() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: solar");

    assert_eq!(accent(&app), Some(Color::Rgb(0, 255, 0)));
    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("theme: solar")
    );
}

#[test]
fn an_installed_theme_may_leave_slots_out() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: solar");

    assert_eq!(
        app.state().theme().style("status.bar"),
        Theme::default().style("status.bar")
    );
}

#[test]
fn the_user_theme_file_is_laid_over_the_installed_theme() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    write(dir.path(), "theme.toml", "[palette]\ntext = \"#010203\"\n");
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: solar");

    assert_eq!(accent(&app), Some(Color::Rgb(0, 255, 0)));
    assert_eq!(
        app.state().theme().style("status.bar").fg,
        Some(Color::Rgb(1, 2, 3))
    );
}

#[test]
fn a_user_theme_file_wins_over_the_installed_theme_where_both_say_something() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#0000ff\"\n",
    );
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: solar");

    assert_eq!(accent(&app), Some(Color::Rgb(0, 0, 255)));
}

#[test]
fn a_broken_installed_theme_is_reported_and_left_out() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "bad", "[palette\n");
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: bad");

    assert!(app.state().notifications().has_errors());
    assert_eq!(*app.state().theme(), Theme::default());
}

#[test]
fn choosing_another_installed_theme_replaces_the_first() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    install(dir.path(), "light", crate::ui::theme::LIGHT_THEME_TOML);
    let mut app = app_with_config_dir(dir.path());
    run_from_palette(&mut app, "Theme: solar");

    run_from_palette(&mut app, "Theme: light");

    assert_eq!(*app.state().theme(), Theme::built_in(ThemeVariant::Light));
}

#[test]
fn a_reload_keeps_the_installed_theme() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());
    run_from_palette(&mut app, "Theme: solar");

    reload(&mut app);

    assert_eq!(accent(&app), Some(Color::Rgb(0, 255, 0)));
}

#[test]
fn saving_the_installed_theme_applies_the_change() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());
    run_from_palette(&mut app, "Theme: solar");
    install(dir.path(), "solar", "[palette]\naccent = \"#0000ff\"\n");

    saved(&mut app, &dir.path().join("themes/solar.toml"));

    assert_eq!(accent(&app), Some(Color::Rgb(0, 0, 255)));
}

#[test]
fn a_theme_that_was_removed_cannot_be_chosen() {
    let dir = tempfile::tempdir().unwrap();
    let mut keymap = crate::keymap::Keymap::defaults(crate::terminal::KeyboardSupport::Enhanced);
    keymap
        .apply_toml("[[binding]]\nkeys = \"f9\"\naction = { use_installed_theme = \"ghost\" }")
        .unwrap();
    let mut app = App::with_keymap(crate::app::state::AppState::default(), keymap)
        .with_config_dir(Some(dir.path().to_path_buf()));

    press(&mut app, KeyCode::F(9));

    assert!(app.state().notifications().has_errors());
}

#[test]
fn theme_choose_opens_the_palette_on_the_installed_themes_only() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: Choose");

    let palette = app.state().palette().unwrap();
    assert_eq!(palette.query(), "Theme: ");
    let titles: Vec<_> = palette.visible().map(|entry| entry.title.clone()).collect();
    assert!(titles.contains(&"Theme: solar".to_owned()));
    for template in ["dark", "light", "soldark", "sollight"] {
        assert!(
            !titles.contains(&format!("Theme: {template}")),
            "{template}"
        );
    }
}

#[test]
fn the_themes_folder_is_watched_so_a_theme_dropped_in_shows_up_on_reopening() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let (tx, _rx) = tokio::sync::mpsc::channel::<crate::event::Event>(16);
    let app = App::default()
        .with_config_dir(Some(dir.path().to_path_buf()))
        .with_watcher(FsWatcher::new(tx).unwrap());

    assert!(
        app.watched_directories()
            .contains(&canonical(&dir.path().join("themes")))
    );
}
