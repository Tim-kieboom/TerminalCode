use super::*;

#[test]
fn a_settings_file_that_cannot_be_made_opens_the_defaults_in_a_locked_tab() {
    let (mut app, dir) = app_with_unwritable_config_dir();

    run_from_palette(&mut app, "Open Settings");

    assert_eq!(editor_text(&app), crate::settings::DEFAULT_SETTINGS_JSONC);
    assert_eq!(app.state().editor().buffer().path(), None);
    assert!(app.state().notifications().has_errors());
    assert!(latest(&app).contains("could not make"), "{}", latest(&app));
    assert!(
        latest(&app).contains("cannot be edited or saved"),
        "{}",
        latest(&app)
    );
    let screen = screen_of(&mut app);
    assert!(
        screen.contains("settings.json (defaults) [read-only]"),
        "{screen}"
    );
    assert!(!dir.path().join("blocker/config").exists());
}

#[test]
fn the_defaults_copy_cannot_be_edited_and_cannot_be_saved() {
    let (mut app, _dir) = app_with_unwritable_config_dir();
    run_from_palette(&mut app, "Open Settings");

    for c in "xyz".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Backspace);
    assert_eq!(editor_text(&app), crate::settings::DEFAULT_SETTINGS_JSONC);
    assert!(!app.state().editor().buffer().is_dirty());

    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
    )));
    assert!(app.state().notifications().has_errors());
    assert!(
        latest(&app).contains("copy of the defaults"),
        "{}",
        latest(&app)
    );
}

#[test]
fn the_defaults_copy_can_still_be_moved_around_selected_and_copied() {
    let (mut app, _dir) = app_with_unwritable_config_dir();
    run_from_palette(&mut app, "Open Settings");
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Right);
    // Dismisses the error that is on screen, then runs the commands.
    press(&mut app, KeyCode::Esc);

    for c in ['a', 'c'] {
        app.handle_input(InputEvent::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::CONTROL,
        )));
    }

    assert!(
        !app.state().notifications().has_errors(),
        "{}",
        latest(&app)
    );
}
