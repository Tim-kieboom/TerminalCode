use super::*;

#[test]
fn saving_the_theme_file_applies_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );

    saved(&mut app, &dir.path().join("theme.toml"));

    let focused = app.state().theme().style("pane.border.focused");
    assert_eq!(focused.fg, Some(Color::Rgb(0, 255, 0)));
}

#[test]
fn saving_the_keymap_file_applies_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(
        dir.path(),
        "keymap.toml",
        "[[binding]]\nkeys = \"ctrl+t\"\naction = \"new_file\"\n",
    );

    saved(&mut app, &dir.path().join("keymap.toml"));
    let tabs = app.state().workspace().tab_names().0.len();
    app.handle_input(crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('t'),
            crossterm::event::KeyModifiers::CONTROL,
        ),
    ));

    assert_eq!(app.state().workspace().tab_names().0.len(), tabs + 1);
}

#[test]
fn saving_the_layout_file_applies_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(dir.path(), "layout.ron", "Pane(view: Editor)");

    saved(&mut app, &dir.path().join("layout.ron"));

    assert!(!app.state().layout().contains(&ComponentKind::Explorer));
}

#[test]
fn another_file_in_the_config_folder_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(dir.path(), "notes.txt", "x");
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );

    saved(&mut app, &dir.path().join("notes.txt"));

    assert_ne!(
        app.state().theme().style("pane.border.focused").fg,
        Some(Color::Rgb(0, 255, 0))
    );
}

#[test]
fn a_file_with_the_name_of_a_config_file_elsewhere_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );
    write(elsewhere.path(), "theme.toml", "x");

    saved(&mut app, &elsewhere.path().join("theme.toml"));

    assert_ne!(
        app.state().theme().style("pane.border.focused").fg,
        Some(Color::Rgb(0, 255, 0))
    );
}

#[test]
fn a_config_file_saved_with_a_mistake_is_reported_and_the_built_in_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(dir.path(), "theme.toml", "[palette\n");

    saved(&mut app, &dir.path().join("theme.toml"));

    assert!(app.state().notifications().has_errors());
    assert_eq!(*app.state().theme(), Theme::default());
}

#[test]
fn without_a_config_folder_a_saved_file_is_just_a_changed_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::default();
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );

    saved(&mut app, &dir.path().join("theme.toml"));

    assert_eq!(*app.state().theme(), Theme::default());
}

#[test]
fn the_config_folder_is_watched_when_it_exists() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(16);
    let app = App::default()
        .with_config_dir(Some(dir.path().to_path_buf()))
        .with_watcher(FsWatcher::new(tx).unwrap());

    assert!(app.watched_directories().contains(&canonical(dir.path())));
}

#[test]
fn a_config_folder_that_does_not_exist_is_not_watched() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("not-there");
    let (tx, _rx) = tokio::sync::mpsc::channel(16);
    let app = App::default()
        .with_config_dir(Some(missing.clone()))
        .with_watcher(FsWatcher::new(tx).unwrap());

    assert!(
        !app.watched_directories()
            .iter()
            .any(|watched| watched.ends_with("not-there"))
    );
}

#[test]
fn a_config_file_that_was_only_read_is_not_reloaded() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );
    let mut app = app_with_config_dir(dir.path());

    // The watcher also reports a file when it is read, and a reload reads it.
    saved(&mut app, &dir.path().join("theme.toml"));

    assert!(
        !app.state()
            .notifications()
            .texts()
            .iter()
            .any(|text| text.contains("reloaded")),
        "nothing was saved, so nothing is reloaded"
    );
}

#[test]
fn reloading_does_not_cause_another_reload() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );
    saved(&mut app, &dir.path().join("theme.toml"));
    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("reloaded")
    );
    app.state_mut().dismiss_errors();
    app.expire_notifications(std::time::Instant::now() + std::time::Duration::from_secs(60));

    saved(&mut app, &dir.path().join("theme.toml"));

    assert_eq!(app.state().latest_notification(), None);
}
