use super::*;

#[test]
fn the_palette_lists_reload() {
    assert!(Action::palette_actions().contains(&Action::Reload));
    assert!(Action::Reload.title().is_some());
}

#[test]
fn reload_applies_a_changed_theme() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    let before = app.state().theme().style("pane.border.focused");
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );

    reload(&mut app);

    let after = app.state().theme().style("pane.border.focused");
    assert_ne!(after, before);
    assert_eq!(after.fg, Some(Color::Rgb(0, 255, 0)));
}

#[test]
fn reload_applies_a_changed_layout() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    assert!(app.state().layout().contains(&ComponentKind::Explorer));
    write(dir.path(), "layout.ron", "Pane(view: Editor)");

    reload(&mut app);

    assert!(!app.state().layout().contains(&ComponentKind::Explorer));
}

#[test]
fn reload_applies_a_changed_keymap() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(
        dir.path(),
        "keymap.toml",
        "[[binding]]\nkeys = \"f2\"\naction = \"quit\"\n",
    );
    press(&mut app, KeyCode::F(2));
    assert!(!app.should_quit(), "not bound before the reload");

    reload(&mut app);
    press(&mut app, KeyCode::F(2));

    assert!(app.should_quit());
}

#[test]
fn reload_says_what_it_did() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());

    reload(&mut app);

    let message = app.state().latest_notification().unwrap();
    assert!(message.contains("reloaded"), "{message}");
}

#[test]
fn a_removed_file_goes_back_to_the_default_on_reload() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    let default = app.state().theme().clone();
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );
    reload(&mut app);
    assert_ne!(*app.state().theme(), default);

    fs::remove_file(dir.path().join("theme.toml")).unwrap();
    reload(&mut app);

    assert_eq!(*app.state().theme(), default);
}

#[test]
fn an_invalid_file_is_reported_and_the_other_files_are_still_applied() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(dir.path(), "theme.toml", "[\"a\"]\ntext = \"@nope\"\n");
    write(dir.path(), "layout.ron", "Pane(view: Editor)");

    reload(&mut app);

    let message = app.state().latest_notification().unwrap();
    assert!(message.contains("theme.toml"), "{message}");
    assert_eq!(*app.state().theme(), Theme::default());
    assert!(!app.state().layout().contains(&ComponentKind::Explorer));
}

#[test]
fn reload_keeps_the_terminal_background_learned_at_startup() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    let mut theme = Theme::default();
    theme.set_terminal_background(Some(Rgb { r: 1, g: 2, b: 3 }));
    app.state_mut().set_theme(theme);

    reload(&mut app);

    assert_eq!(
        app.state().theme().terminal_background(),
        Some(Rgb { r: 1, g: 2, b: 3 })
    );
}

#[test]
fn the_keyboard_leaves_a_component_the_new_layout_lacks() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    app.state_mut().set_focus(Focus::Explorer);
    write(dir.path(), "layout.ron", "Pane(view: Editor)");

    reload(&mut app);

    assert_eq!(app.state().focus(), Focus::Editor);
}

#[test]
fn reload_leaves_open_files_alone() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    press(&mut app, KeyCode::Char('x'));
    let text = editor_text(&app);

    reload(&mut app);

    assert_eq!(editor_text(&app), text);
}

#[test]
fn reload_with_a_syntax_worker_running_starts_a_new_one_without_trouble() {
    let dir = tempfile::tempdir().unwrap();
    let (events, _results) = tokio::sync::mpsc::channel(16);
    let mut app = app_with_config_dir(dir.path()).with_events(events);
    write(dir.path(), "theme.toml", "[palette]\nteal = \"#00ff00\"\n");

    reload(&mut app);

    assert!(app.state_mut().refresh_highlights().is_empty());
    let message = app.state().latest_notification().unwrap();
    assert!(message.contains("reloaded"), "{message}");
}
