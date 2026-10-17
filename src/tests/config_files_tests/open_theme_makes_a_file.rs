use super::*;

#[test]
fn a_theme_made_from_a_template_is_chosen_so_its_edits_show() {
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
fn opening_the_theme_twice_shows_one_tab() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "solar", GREEN_ACCENT);
    let mut app = app_with_config_dir(dir.path());
    run_from_palette(&mut app, "Theme: solar");
    run_from_palette(&mut app, "Open Theme");
    let tabs = app.state().workspace().tab_names().0.len();

    run_from_palette(&mut app, "Open Theme");

    assert_eq!(app.state().workspace().tab_names().0.len(), tabs);
}
