use super::*;

#[test]
fn switching_to_a_tab_whose_file_changed_unnoticed_reloads_it() {
    let (mut app, _dir, path) = app_with_a_tab_behind("old\n");
    fs::write(&path, "new\n").unwrap();

    next_tab(&mut app);

    assert_eq!(text(&app), "new\n");
    assert!(
        latest(&app).contains("a.txt changed on disk; reloaded it"),
        "{}",
        latest(&app)
    );
}

#[test]
fn switching_to_a_modified_tab_whose_file_changed_keeps_the_edits_and_says_so() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    ctrl(&mut app, 'n');
    fs::write(&path, "theirs\n").unwrap();

    next_tab(&mut app);

    assert_eq!(text(&app), "mine old\n");
    assert!(app.state().notifications().has_errors());
    assert!(
        latest(&app).contains("your changes are kept"),
        "{}",
        latest(&app)
    );
}

#[test]
fn switching_to_a_tab_whose_file_is_gone_says_so() {
    let (mut app, _dir, path) = app_with_a_tab_behind("old\n");
    fs::remove_file(&path).unwrap();

    next_tab(&mut app);

    assert_eq!(text(&app), "old\n");
    assert!(
        latest(&app).contains("a.txt was deleted on disk"),
        "{}",
        latest(&app)
    );
}

#[test]
fn switching_to_a_tab_that_did_not_change_says_nothing() {
    let (mut app, _dir, _path) = app_with_a_tab_behind("old\n");
    let before = notifications(&app);

    next_tab(&mut app);

    assert_eq!(notifications(&app), before);
}

#[test]
fn a_change_that_was_already_reported_is_not_reported_again_on_the_next_switch() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    ctrl(&mut app, 'n');
    fs::write(&path, "theirs\n").unwrap();
    next_tab(&mut app);
    assert!(app.state().notifications().has_errors());
    app.handle_input(key(KeyCode::Char('x'), KeyModifiers::NONE));
    // The error is dismissed by the key; go away and come back.
    next_tab(&mut app);
    let before = notifications(&app);

    next_tab(&mut app);

    assert_eq!(notifications(&app), before);
}

#[test]
fn only_the_tab_that_came_to_the_front_is_looked_at() {
    let (mut app, _dir, path) = app_with_file("old\n");
    ctrl(&mut app, 'n');
    ctrl(&mut app, 'n');
    fs::write(&path, "new\n").unwrap();

    // The untitled tabs have no file; typing in front does not look at a.txt.
    type_str(&mut app, "x");

    assert!(!latest(&app).contains("a.txt"));
}

#[test]
fn getting_the_screen_back_looks_at_every_open_file() {
    let (mut app, dir, path) = app_with_a_tab_behind("old\n");
    let other = dir.path().join("b.txt");
    fs::write(&other, "b old\n").unwrap();
    app.state_mut().workspace_mut().open_path(&other).unwrap();
    fs::write(&path, "new a\n").unwrap();
    fs::write(&other, "new b\n").unwrap();

    app.handle_input(InputEvent::FocusGained);

    assert_eq!(text(&app), "new b\n");
    // The tabs are a.txt, the untitled one and b.txt; next wraps to a.txt. Its
    // file was reloaded already, so the switch has nothing left to report.
    let before = notifications(&app);
    next_tab(&mut app);
    assert_eq!(text(&app), "new a\n");
    assert_eq!(notifications(&app), before);
}

#[test]
fn getting_the_screen_back_with_nothing_changed_says_nothing_and_does_not_redraw() {
    let (mut app, _dir, _path) = app_with_a_tab_behind("old\n");
    let _ = screen(&mut app);
    app.mark_drawn();
    let before = notifications(&app);

    app.handle_input(InputEvent::FocusGained);

    assert_eq!(notifications(&app), before);
    assert!(!app.needs_redraw());
}

#[test]
fn losing_the_screen_does_nothing() {
    let (mut app, _dir, path) = app_with_a_tab_behind("old\n");
    fs::write(&path, "new\n").unwrap();

    app.handle_input(InputEvent::FocusLost);

    assert_eq!(app.state().workspace().tab_names().1, 1);
    assert!(!latest(&app).contains("changed on disk"));
}

#[test]
fn closing_the_front_tab_looks_at_the_one_that_comes_to_the_front() {
    let (mut app, _dir, path) = app_with_a_tab_behind("old\n");
    fs::write(&path, "new\n").unwrap();

    ctrl(&mut app, 'w');

    assert_eq!(text(&app), "new\n");
}

#[test]
fn clicking_a_tab_looks_at_its_file() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let (mut app, _dir, path) = app_with_a_tab_behind("old\n");
    let _ = screen(&mut app);
    fs::write(&path, "new\n").unwrap();
    let tab = app.state().workspace().tab_areas()[0];

    app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: tab.x + 1,
        row: tab.y,
        modifiers: KeyModifiers::NONE,
    }));

    assert_eq!(text(&app), "new\n");
}

#[test]
fn the_tab_says_when_its_file_changed_under_unsaved_edits() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    fs::write(&path, "theirs\n").unwrap();
    changed(&mut app, &path);

    let screen = screen(&mut app);

    assert!(screen.contains("a.txt [+] [changed]"), "{screen}");
}

#[test]
fn the_tab_says_when_its_file_is_gone() {
    let (mut app, _dir, path) = app_with_file("old\n");
    fs::remove_file(&path).unwrap();
    changed(&mut app, &path);

    let screen = screen(&mut app);

    assert!(screen.contains("a.txt [deleted]"), "{screen}");
}

#[test]
fn the_note_goes_when_the_file_is_saved_again() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    fs::write(&path, "theirs\n").unwrap();
    changed(&mut app, &path);
    ctrl(&mut app, 's');
    ctrl(&mut app, 's');

    let screen = screen(&mut app);

    assert!(!screen.contains("[changed]"), "{screen}");
    assert!(screen.contains("a.txt"), "{screen}");
}

#[test]
fn the_note_goes_when_the_file_is_back_as_it_was() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    fs::write(&path, "theirs\n").unwrap();
    changed(&mut app, &path);
    fs::write(&path, "old\n").unwrap();
    changed(&mut app, &path);

    let screen = screen(&mut app);

    assert!(!screen.contains("[changed]"), "{screen}");
}

#[test]
fn a_tab_with_no_change_has_no_note() {
    let (mut app, _dir, _path) = app_with_file("old\n");

    let screen = screen(&mut app);

    assert!(
        !screen.contains("[changed]") && !screen.contains("[deleted]"),
        "{screen}"
    );
}
