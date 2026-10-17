use super::*;

#[test]
fn theme_new_lists_the_built_in_themes_and_the_installed_ones_as_templates() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: New...");

    let palette = app.state().palette().unwrap();
    assert_eq!(palette.query(), "Theme: New from ");
    let titles: Vec<_> = palette.visible().map(|entry| entry.title.clone()).collect();
    assert_eq!(
        titles,
        [
            "Theme: New from Dark",
            "Theme: New from Light",
            "Theme: New from SolDark",
            "Theme: New from SolLight",
            "Theme: New from solar"
        ]
    );
}

#[test]
fn copies_of_the_built_in_themes_are_not_offered_twice() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "dark", GREEN_ACCENT);
    install(dir.path(), "Light", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: New...");

    let count = app.state().palette().unwrap().visible().count();
    assert_eq!(count, 4);
}

#[test]
fn choosing_a_template_asks_for_a_name() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Theme: New...");
    type_str(&mut app, "Light");
    enter(&mut app);

    let prompt = app.state().name_prompt().unwrap();
    assert_eq!(prompt.name(), "");
}

#[test]
fn a_name_makes_the_theme_from_the_template_chooses_it_and_opens_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());

    new_theme(&mut app, "Light", "paper");

    let path = dir.path().join("themes/paper.toml");
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        crate::ui::theme::LIGHT_THEME_TOML
    );
    assert_eq!(*app.state().theme(), Theme::built_in(ThemeVariant::Light));
    assert_eq!(app.state().editor().buffer().path(), Some(path.as_path()));
    assert!(app.state().name_prompt().is_none());
    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("themes/paper.toml")
    );
}

#[test]
fn the_new_theme_is_the_one_in_use_so_saving_it_applies_the_change() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    new_theme(&mut app, "Dark", "mine");
    write(
        dir.path(),
        "themes/mine.toml",
        &format!("{DEFAULT_THEME_TOML}\n[\"my.slot\"]\ntext = \"red\"\n"),
    );

    saved(&mut app, &dir.path().join("themes/mine.toml"));

    assert_eq!(app.state().theme().style("my.slot").fg, Some(Color::Red));
}

#[test]
fn an_installed_theme_can_be_the_template() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());

    new_theme(&mut app, "solar", "solar2");

    assert_eq!(
        fs::read_to_string(dir.path().join("themes/solar2.toml")).unwrap(),
        GREEN_ACCENT
    );
    assert_eq!(accent(&app), Some(Color::Rgb(0, 255, 0)));
}

#[test]
fn a_name_may_end_in_toml() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());

    new_theme(&mut app, "Dark", "mine.toml");

    assert!(dir.path().join("themes/mine.toml").is_file());
    assert!(!dir.path().join("themes/mine.toml.toml").exists());
}

#[test]
fn a_name_that_is_taken_is_refused_and_the_prompt_stays() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "mine", "# old\n");
    let mut app = app_with_config_dir(dir.path());

    new_theme(&mut app, "Dark", "mine");

    assert_eq!(
        fs::read_to_string(dir.path().join("themes/mine.toml")).unwrap(),
        "# old\n"
    );
    assert!(app.state().name_prompt().is_some());
    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("already exists")
    );
}

#[test]
fn bad_names_are_refused() {
    for name in ["", "  ", "a/b", "..", ".hidden", "../x"] {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_config_dir(dir.path());

        new_theme(&mut app, "Dark", name);

        assert!(app.state().name_prompt().is_some(), "{name:?}");
        assert!(
            crate::config::installed_themes(dir.path()).is_empty(),
            "{name:?}"
        );
    }
}

#[test]
fn escape_at_the_name_makes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    run_from_palette(&mut app, "Theme: New...");
    type_str(&mut app, "Dark");
    enter(&mut app);
    type_str(&mut app, "mine");

    press(&mut app, KeyCode::Esc);

    assert!(app.state().name_prompt().is_none());
    assert!(!dir.path().join("themes").exists());
}

#[test]
fn the_new_theme_shows_up_in_the_theme_list() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    new_theme(&mut app, "Dark", "mine");

    assert_eq!(crate::config::installed_themes(dir.path()), ["mine"]);
}

#[test]
fn without_a_config_folder_there_is_no_name_to_ask_for() {
    let mut app = App::default();

    run_from_palette(&mut app, "New from Dark");

    assert!(app.state().name_prompt().is_none());
    assert!(app.state().notifications().has_errors());
}

#[test]
fn a_template_that_vanished_makes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());
    run_from_palette(&mut app, "Theme: New...");
    type_str(&mut app, "solar");
    enter(&mut app);
    fs::remove_file(dir.path().join("themes/solar.toml")).unwrap();
    type_str(&mut app, "copy");

    enter(&mut app);

    assert!(!dir.path().join("themes/copy.toml").exists());
    assert!(app.state().notifications().has_errors());
}

#[test]
fn the_palette_offers_theme_new() {
    assert!(Action::palette_actions().contains(&Action::NewTheme));
    assert!(Action::NewTheme.title().is_some());
}
