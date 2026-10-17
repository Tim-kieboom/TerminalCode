use super::*;

#[test]
fn typing_into_a_read_only_file_changes_nothing_and_says_why() {
    let (mut app, _dir, _path) = app_with_read_only_file("old\n");

    type_str(&mut app, "x");

    assert_eq!(text(&app), "old\n");
    assert!(!app.state().editor().buffer().is_dirty());
    assert!(app.state().notifications().has_errors());
    assert!(latest(&app).contains("read-only"), "{}", latest(&app));
}

#[test]
fn every_kind_of_edit_is_locked_in_a_read_only_file() {
    let (mut app, _dir, _path) = app_with_read_only_file("old\nline\n");

    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Delete);
    press(&mut app, KeyCode::End);
    press(&mut app, KeyCode::Backspace);
    ctrl(&mut app, 'x');

    assert_eq!(text(&app), "old\nline\n");
    assert!(!app.state().editor().buffer().is_dirty());
}

#[test]
fn moving_selecting_and_copying_still_work_in_a_read_only_file() {
    let (mut app, _dir, _path) = app_with_read_only_file("old\n");

    press(&mut app, KeyCode::Right);
    ctrl(&mut app, 'a');
    ctrl(&mut app, 'c');

    assert!(
        !app.state().notifications().has_errors(),
        "{}",
        latest(&app)
    );
    assert_eq!(text(&app), "old\n");
}

#[test]
fn a_file_that_turns_read_only_under_unsaved_edits_locks_new_edits_but_keeps_undo() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    set_read_only(&path, true);
    changed(&mut app, &path);

    type_str(&mut app, "more ");
    assert_eq!(text(&app), "mine old\n", "new edits are locked");

    ctrl(&mut app, 'z');
    assert_eq!(text(&app), "old\n", "undo still works");

    ctrl(&mut app, 'y');
    assert_eq!(text(&app), "mine old\n", "so does redo");
}

#[test]
fn editing_works_again_when_the_file_is_writable_again() {
    let (mut app, _dir, path) = app_with_read_only_file("old\n");
    type_str(&mut app, "x");
    assert_eq!(text(&app), "old\n");

    set_read_only(&path, false);
    changed(&mut app, &path);
    type_str(&mut app, "x");

    assert_eq!(text(&app), "xold\n");
}
