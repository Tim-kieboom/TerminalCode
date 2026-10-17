use super::*;

#[test]
fn the_palette_lists_the_three_config_files() {
    let actions = Action::palette_actions();

    for file in [ConfigFile::Theme, ConfigFile::Keymap, ConfigFile::Layout] {
        assert!(actions.contains(&Action::OpenConfig(file)), "{file:?}");
        assert!(Action::OpenConfig(file).title().is_some());
    }
}

#[test]
fn with_no_theme_chosen_open_theme_makes_nothing_and_says_how_to_make_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Open Theme");

    let message = app.state().latest_notification().unwrap();
    assert!(message.contains("Theme: New"), "{message}");
    assert!(!dir.path().join("themes").exists());
    assert!(!dir.path().join("theme.toml").exists());
    assert!(!app.state().workspace().has_tabs() || app.state().editor().buffer().path().is_none());
}

#[test]
fn each_file_starts_from_its_own_default() {
    let cases = [
        ("Open Keymap", "keymap.toml", DEFAULT_KEYMAP_TOML),
        ("Open Layout", "layout.ron", DEFAULT_LAYOUT_RON),
    ];

    for (query, name, default) in cases {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_config_dir(dir.path());

        run_from_palette(&mut app, query);

        assert_eq!(fs::read_to_string(dir.path().join(name)).unwrap(), default);
    }
}

#[test]
fn the_theme_file_in_use_is_opened_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "mine", "[palette]\naccent = \"red\"\n");
    let mut app = app_with_config_dir(dir.path());
    run_from_palette(&mut app, "Theme: mine");

    run_from_palette(&mut app, "Open Theme");

    assert_eq!(editor_text(&app), "[palette]\naccent = \"red\"\n");
    assert_eq!(
        fs::read_to_string(dir.path().join("themes/mine.toml")).unwrap(),
        "[palette]\naccent = \"red\"\n"
    );
}

#[test]
fn the_config_directory_is_made_when_it_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("a").join("terminalcode");
    let mut app = app_with_config_dir(&config);

    run_from_palette(&mut app, "Open Layout");

    assert!(config.join("layout.ron").is_file());
}

#[test]
fn opening_an_existing_file_twice_shows_the_same_tab() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Open Layout");
    let after_first = app.state().workspace().tab_names().0.len();

    run_from_palette(&mut app, "Open Layout");

    assert_eq!(app.state().workspace().tab_names().0.len(), after_first);
}

#[test]
fn without_a_config_directory_it_says_so_and_makes_nothing() {
    let mut app = App::default();

    run_from_palette(&mut app, "Open Theme");

    let message = app.state().latest_notification().unwrap();
    assert!(message.contains("no config directory"), "{message}");
}

#[test]
fn a_directory_that_cannot_be_made_is_reported_with_its_path() {
    let dir = tempfile::tempdir().unwrap();
    // A file where the config directory should be.
    let blocker = dir.path().join("terminalcode");
    fs::write(&blocker, "").unwrap();
    let mut app = app_with_config_dir(&blocker);

    run_from_palette(&mut app, "Open Layout");

    let message = app.state().latest_notification().unwrap();
    assert!(message.contains("terminalcode"), "{message}");
}

#[test]
fn an_action_can_be_bound_in_a_keymap_file() {
    let action: Action = toml::from_str::<Wrapper>("action = { open_config = \"theme\" }")
        .unwrap()
        .action;

    assert_eq!(action, Action::OpenConfig(ConfigFile::Theme));
}
