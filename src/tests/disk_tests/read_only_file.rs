use super::*;

#[test]
fn the_tab_says_when_its_file_is_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "old\n").unwrap();
    set_read_only(&path, true);
    let mut app = App::new(initial_state(std::slice::from_ref(&path)).unwrap());

    let screen = screen(&mut app);

    assert!(screen.contains("a.txt [read-only]"), "{screen}");
}

#[cfg(not(target_os = "linux"))]
#[test]
fn saving_a_read_only_file_is_refused_every_time_and_the_text_stays() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    set_read_only(&path, true);

    ctrl(&mut app, 's');
    ctrl(&mut app, 's');

    assert_eq!(fs::read_to_string(&path).unwrap(), "old\n");
    assert!(app.state().notifications().has_errors());
    assert!(latest(&app).contains("read-only"), "{}", latest(&app));
    assert!(app.state().editor().buffer().is_dirty());
    assert_eq!(text(&app), "mine old\n");
}

#[cfg(not(target_os = "linux"))]
#[test]
fn a_read_only_file_that_also_changed_on_disk_is_refused_not_asked_about() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    fs::write(&path, "theirs\n").unwrap();
    set_read_only(&path, true);

    ctrl(&mut app, 's');
    ctrl(&mut app, 's');

    assert_eq!(fs::read_to_string(&path).unwrap(), "theirs\n");
    assert!(latest(&app).contains("read-only"), "{}", latest(&app));
}

// On Linux a save that the file permissions refuse asks for the root password
// instead (see `root_save_tests`); giving up leaves the file and the text alone.
#[cfg(target_os = "linux")]
#[test]
fn saving_a_read_only_file_asks_for_the_root_password_and_giving_up_leaves_everything() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    set_read_only(&path, true);

    ctrl(&mut app, 's');
    assert!(app.state().root_prompt().is_some());
    app.handle_input(key(KeyCode::Esc, KeyModifiers::NONE));
    ctrl(&mut app, 's');
    assert!(app.state().root_prompt().is_some());
    app.handle_input(key(KeyCode::Esc, KeyModifiers::NONE));

    assert_eq!(fs::read_to_string(&path).unwrap(), "old\n");
    assert!(app.state().editor().buffer().is_dirty());
    assert_eq!(text(&app), "mine old\n");
}

#[cfg(target_os = "linux")]
#[test]
fn a_read_only_file_that_also_changed_on_disk_asks_for_the_password_not_whether_to_overwrite() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    fs::write(&path, "theirs\n").unwrap();
    set_read_only(&path, true);

    ctrl(&mut app, 's');

    assert!(app.state().root_prompt().is_some());
    assert_eq!(fs::read_to_string(&path).unwrap(), "theirs\n");
}

#[test]
fn the_note_follows_the_permissions_while_the_editor_runs() {
    let (mut app, _dir, path) = app_with_file("old\n");
    set_read_only(&path, true);
    changed(&mut app, &path);

    assert!(latest(&app).contains("read-only now"), "{}", latest(&app));
    let shown = screen(&mut app);
    assert!(shown.contains("a.txt [read-only]"), "{shown}");

    set_read_only(&path, false);
    changed(&mut app, &path);

    assert!(latest(&app).contains("writable again"), "{}", latest(&app));
    let shown = screen(&mut app);
    assert!(!shown.contains("[read-only]"), "{shown}");

    type_str(&mut app, "mine ");
    ctrl(&mut app, 's');
    assert_eq!(fs::read_to_string(&path).unwrap(), "mine old\n");
}

#[test]
fn a_permission_change_nobody_reported_is_found_when_the_screen_comes_back() {
    let (mut app, _dir, path) = app_with_file("old\n");
    set_read_only(&path, true);

    app.handle_input(InputEvent::FocusGained);

    let shown = screen(&mut app);
    assert!(shown.contains("a.txt [read-only]"), "{shown}");
}

#[test]
fn a_writable_file_has_no_read_only_note() {
    let (mut app, _dir, _path) = app_with_file("old\n");

    let shown = screen(&mut app);

    assert!(!shown.contains("[read-only]"), "{shown}");
}
