use super::*;

#[test]
fn saving_a_document_whose_file_was_deleted_asks_first_and_the_second_save_recreates_it() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    fs::remove_file(&path).unwrap();

    ctrl(&mut app, 's');

    assert!(!path.exists(), "nothing written yet");
    assert!(app.state().notifications().has_errors());
    assert!(
        latest(&app).contains("deleted on disk") && latest(&app).contains("save again"),
        "{}",
        latest(&app)
    );

    ctrl(&mut app, 's');

    assert_eq!(fs::read_to_string(&path).unwrap(), "mine old\n");
    assert!(!app.state().editor().buffer().is_dirty());
    let screen = self::screen(&mut app);
    assert!(!screen.contains("[deleted]"), "{screen}");
}

#[test]
fn doing_anything_else_between_two_saves_of_a_deleted_file_asks_again() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    fs::remove_file(&path).unwrap();
    ctrl(&mut app, 's');

    type_str(&mut app, "more ");
    ctrl(&mut app, 's');

    assert!(!path.exists());
}

#[test]
fn saving_a_deleted_file_in_a_deleted_folder_creates_no_folder() {
    let (mut app, dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    fs::remove_dir_all(dir.path()).unwrap();

    ctrl(&mut app, 's');
    ctrl(&mut app, 's');

    assert!(!dir.path().exists(), "the folder was made again");
    assert!(!path.exists());
    assert!(app.state().notifications().has_errors());
    assert!(
        latest(&app).contains("folder"),
        "the error says the folder is gone: {}",
        latest(&app)
    );
    assert!(app.state().editor().buffer().is_dirty());
    assert_eq!(text(&app), "mine old\n");
}

#[test]
fn the_deleted_note_stays_on_a_modified_tab_and_after_a_tab_switch() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    fs::remove_file(&path).unwrap();
    changed(&mut app, &path);
    ctrl(&mut app, 'n');
    next_tab(&mut app);

    let screen = self::screen(&mut app);

    assert!(screen.contains("a.txt [+] [deleted]"), "{screen}");
    assert_eq!(text(&app), "mine old\n");
}

#[test]
fn a_file_that_comes_back_with_the_same_content_clears_the_note_without_a_word() {
    let (mut app, _dir, path) = app_with_file("old\n");
    fs::remove_file(&path).unwrap();
    changed(&mut app, &path);
    let before = notifications(&app);

    fs::write(&path, "old\n").unwrap();
    changed(&mut app, &path);

    assert_eq!(notifications(&app), before, "{}", latest(&app));
    let screen = self::screen(&mut app);
    assert!(!screen.contains("[deleted]"), "{screen}");
}

#[test]
fn a_file_that_comes_back_different_reloads_a_clean_tab_and_clears_the_note() {
    let (mut app, _dir, path) = app_with_file("old\n");
    fs::remove_file(&path).unwrap();
    changed(&mut app, &path);

    fs::write(&path, "new\n").unwrap();
    changed(&mut app, &path);

    assert_eq!(text(&app), "new\n");
    assert!(!app.state().editor().buffer().is_dirty());
    let screen = self::screen(&mut app);
    assert!(!screen.contains("[deleted]"), "{screen}");
}

#[test]
fn a_file_that_comes_back_different_under_unsaved_edits_is_a_conflict() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    fs::remove_file(&path).unwrap();
    changed(&mut app, &path);

    fs::write(&path, "theirs\n").unwrap();
    changed(&mut app, &path);

    assert_eq!(text(&app), "mine old\n");
    assert!(app.state().notifications().has_errors());
    let screen = self::screen(&mut app);
    assert!(screen.contains("a.txt [+] [changed]"), "{screen}");
    assert!(!screen.contains("[deleted]"), "{screen}");

    ctrl(&mut app, 's');
    assert_eq!(fs::read_to_string(&path).unwrap(), "theirs\n", "asks first");
    ctrl(&mut app, 's');
    assert_eq!(fs::read_to_string(&path).unwrap(), "mine old\n");
}

#[test]
fn a_deleted_file_is_found_without_any_report_when_the_screen_comes_back() {
    let (mut app, _dir, path) = app_with_file("old\n");
    fs::remove_file(&path).unwrap();

    app.handle_input(InputEvent::FocusGained);

    assert!(latest(&app).contains("deleted on disk"), "{}", latest(&app));
    let screen = self::screen(&mut app);
    assert!(screen.contains("a.txt [deleted]"), "{screen}");

    fs::write(&path, "old\n").unwrap();
    let before = notifications(&app);
    app.handle_input(InputEvent::FocusGained);

    assert_eq!(notifications(&app), before);
    let screen = self::screen(&mut app);
    assert!(!screen.contains("[deleted]"), "{screen}");
}

#[test]
fn a_new_file_that_does_not_exist_yet_is_not_marked_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fresh.txt");
    let mut app = App::new(initial_state(std::slice::from_ref(&path)).unwrap());
    let before = notifications(&app);

    changed(&mut app, &path);
    app.handle_input(InputEvent::FocusGained);

    assert_eq!(notifications(&app), before);
    let screen = self::screen(&mut app);
    assert!(!screen.contains("[deleted]"), "{screen}");
}
