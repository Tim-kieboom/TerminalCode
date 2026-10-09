use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::AppState;
use crate::buffer::Position;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::initial_state;
use crate::watcher::FsWatcher;

fn key(code: KeyCode, modifiers: KeyModifiers) -> InputEvent {
    InputEvent::Key(KeyEvent::new(code, modifiers))
}

fn ctrl(app: &mut App, c: char) {
    app.handle_input(key(KeyCode::Char(c), KeyModifiers::CONTROL));
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        app.handle_input(key(KeyCode::Char(c), KeyModifiers::NONE));
    }
}

fn changed(app: &mut App, path: &Path) {
    app.handle_event(Event::FilesChanged(vec![path.to_path_buf()]));
}

/// An app with `a.txt` (containing `text`) open, and the file's path.
fn app_with_file(text: &str) -> (App, tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, text).unwrap();
    let app = App::new(initial_state(std::slice::from_ref(&path)).unwrap());
    (app, dir, path)
}

fn text(app: &App) -> String {
    app.state().editor().buffer().text()
}

fn notifications(app: &App) -> usize {
    app.state().notifications().len()
}

fn latest(app: &App) -> String {
    app.state()
        .latest_notification()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn an_unmodified_document_reloads_when_its_file_changes() {
    let (mut app, _dir, path) = app_with_file("old\n");

    fs::write(&path, "new\n").unwrap();
    changed(&mut app, &path);

    assert_eq!(text(&app), "new\n");
    assert!(!app.state().editor().buffer().is_dirty());
    assert!(
        latest(&app).contains("a.txt changed on disk"),
        "{}",
        latest(&app)
    );
    assert!(!app.state().notifications().has_errors());
}

#[test]
fn a_reload_keeps_the_cursor_on_the_nearest_place_that_still_exists() {
    let (mut app, _dir, path) = app_with_file("one\ntwo\nthree\n");
    ctrl(&mut app, 'n');
    app.state_mut().workspace_mut().activate_tab(0);
    app.handle_input(key(KeyCode::End, KeyModifiers::CONTROL));
    assert_eq!(
        app.state().editor().selections().primary().head(),
        Position::new(3, 0)
    );

    fs::write(&path, "x\n").unwrap();
    changed(&mut app, &path);

    assert_eq!(
        app.state().editor().selections().primary().head(),
        Position::new(1, 0)
    );
}

#[test]
fn every_view_of_a_reloaded_document_stays_valid() {
    let (mut app, _dir, path) = app_with_file("one\ntwo\nthree\n");
    app.handle_input(key(KeyCode::End, KeyModifiers::CONTROL));
    ctrl(&mut app, 'k');
    app.handle_input(key(KeyCode::Right, KeyModifiers::NONE));

    fs::write(&path, "x\n").unwrap();
    changed(&mut app, &path);

    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    assert_eq!(app.state().workspace().document_count(), 1);
    assert_eq!(text(&app), "x\n");
}

#[test]
fn a_modified_document_keeps_its_text_and_gets_a_sticky_error() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");

    fs::write(&path, "theirs\n").unwrap();
    changed(&mut app, &path);

    assert_eq!(text(&app), "mine old\n");
    assert!(app.state().editor().buffer().is_dirty());
    assert!(app.state().notifications().has_errors());
    assert!(
        latest(&app).contains("a.txt changed on disk"),
        "{}",
        latest(&app)
    );
    assert!(
        latest(&app).contains("your changes are kept"),
        "{}",
        latest(&app)
    );
}

#[test]
fn the_same_change_reported_again_is_not_repeated() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    fs::write(&path, "theirs\n").unwrap();

    changed(&mut app, &path);
    changed(&mut app, &path);
    changed(&mut app, &path);

    assert_eq!(notifications(&app), 1);
}

#[test]
fn a_further_change_is_reported_again() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    fs::write(&path, "theirs\n").unwrap();
    changed(&mut app, &path);
    app.state_mut().dismiss_errors();
    assert_eq!(notifications(&app), 0);

    fs::write(&path, "theirs, edited again\n").unwrap();
    changed(&mut app, &path);

    assert!(app.state().notifications().has_errors());
}

#[test]
fn the_editors_own_save_is_not_reported_as_a_change() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    ctrl(&mut app, 's');
    let before = notifications(&app);

    changed(&mut app, &path);

    assert_eq!(notifications(&app), before);
    assert_eq!(text(&app), "mine old\n");
}

#[test]
fn saving_over_a_changed_file_asks_first_and_the_second_save_overwrites() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    fs::write(&path, "theirs\n").unwrap();
    changed(&mut app, &path);

    ctrl(&mut app, 's');

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "theirs\n",
        "nothing written yet"
    );
    assert!(app.state().notifications().has_errors());
    assert!(
        latest(&app).contains("save again to overwrite"),
        "{}",
        latest(&app)
    );

    ctrl(&mut app, 's');

    assert_eq!(fs::read_to_string(&path).unwrap(), "mine old\n");
    assert!(!app.state().editor().buffer().is_dirty());
    assert!(latest(&app).contains("saved"), "{}", latest(&app));
}

#[test]
fn the_check_happens_at_save_time_even_if_no_event_arrived() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    fs::write(&path, "theirs\n").unwrap();

    ctrl(&mut app, 's');

    assert_eq!(fs::read_to_string(&path).unwrap(), "theirs\n");
    assert!(
        latest(&app).contains("save again to overwrite"),
        "{}",
        latest(&app)
    );
}

#[test]
fn doing_anything_else_between_two_saves_asks_again() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    fs::write(&path, "theirs\n").unwrap();
    ctrl(&mut app, 's');

    type_str(&mut app, "more ");
    ctrl(&mut app, 's');

    assert_eq!(fs::read_to_string(&path).unwrap(), "theirs\n");
    assert!(
        latest(&app).contains("save again to overwrite"),
        "{}",
        latest(&app)
    );
}

#[test]
fn a_deleted_file_is_mentioned_once_and_the_text_stays() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    fs::remove_file(&path).unwrap();

    changed(&mut app, &path);
    changed(&mut app, &path);

    assert_eq!(text(&app), "mine old\n");
    assert_eq!(notifications(&app), 1);
    assert!(latest(&app).contains("deleted on disk"), "{}", latest(&app));
}

#[test]
fn saving_a_document_whose_file_was_deleted_recreates_it_without_asking() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    fs::remove_file(&path).unwrap();

    ctrl(&mut app, 's');

    assert_eq!(fs::read_to_string(&path).unwrap(), "mine old\n");
}

#[test]
fn a_path_spelled_differently_still_finds_the_document() {
    let (mut app, dir, path) = app_with_file("old\n");
    fs::write(&path, "new\n").unwrap();

    changed(&mut app, &dir.path().join(".").join("a.txt"));

    assert_eq!(text(&app), "new\n");
}

#[test]
fn a_change_to_an_unrelated_file_does_nothing() {
    let (mut app, dir, path) = app_with_file("old\n");
    fs::write(&path, "new\n").unwrap();
    let other = dir.path().join("other.txt");
    fs::write(&other, "x").unwrap();

    changed(&mut app, &other);

    assert_eq!(
        text(&app),
        "old\n",
        "only the changed file's document is looked at"
    );
    assert_eq!(notifications(&app), 0);
}

#[test]
fn a_file_that_became_unreadable_is_an_error_and_the_text_stays() {
    let (mut app, _dir, path) = app_with_file("old\n");
    fs::write(&path, [0xff, 0xfe]).unwrap();

    changed(&mut app, &path);

    assert_eq!(text(&app), "old\n");
    assert!(app.state().notifications().has_errors());
}

#[test]
fn quitting_with_a_conflicting_file_asks_before_overwriting_too() {
    let (mut app, _dir, path) = app_with_file("old\n");
    type_str(&mut app, "mine ");
    fs::write(&path, "theirs\n").unwrap();
    ctrl(&mut app, 'q');

    app.handle_input(key(KeyCode::Char('s'), KeyModifiers::NONE));
    assert!(!app.should_quit());
    assert_eq!(fs::read_to_string(&path).unwrap(), "theirs\n");
    assert!(
        latest(&app).contains("save again to overwrite"),
        "{}",
        latest(&app)
    );

    app.handle_input(key(KeyCode::Char('s'), KeyModifiers::NONE));
    assert!(app.should_quit());
    assert_eq!(fs::read_to_string(&path).unwrap(), "mine old\n");
}

#[test]
fn a_watcher_problem_is_shown_as_an_error() {
    let mut app = App::default();

    app.handle_event(Event::WatchFailed("too many open files".to_owned()));

    assert!(app.state().notifications().has_errors());
    assert!(latest(&app).contains("too many open files"));
}

fn app_with_project(dir: &Path) -> App {
    let mut state = AppState::default();
    state.open_project(dir).unwrap();
    App::new(state)
}

#[test]
fn the_explorer_refreshes_when_something_changes_inside_the_project() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "").unwrap();
    let mut app = app_with_project(dir.path());
    fs::write(dir.path().join("fresh.txt"), "").unwrap();

    changed(&mut app, &dir.path().join("fresh.txt"));

    assert!(
        app.state()
            .explorer()
            .rows()
            .iter()
            .any(|row| row.name == "fresh.txt")
    );
}

#[test]
fn a_change_outside_the_project_leaves_the_explorer_alone() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let mut app = app_with_project(dir.path());
    fs::write(dir.path().join("fresh.txt"), "").unwrap();

    changed(&mut app, &elsewhere.path().join("x.txt"));

    assert!(
        !app.state()
            .explorer()
            .rows()
            .iter()
            .any(|row| row.name == "fresh.txt")
    );
}

fn app_with_watcher(app: App) -> (App, mpsc::Receiver<Event>) {
    let (tx, rx) = mpsc::channel(64);
    let watcher = FsWatcher::new(tx).unwrap();
    (app.with_watcher(watcher), rx)
}

fn canon(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap()
}

#[test]
fn the_watched_directories_follow_open_files_and_the_explorers_open_directories() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub/inner.txt"), "").unwrap();
    let other = tempfile::tempdir().unwrap();
    let file = other.path().join("far.txt");
    fs::write(&file, "").unwrap();
    let (mut app, _rx) = app_with_watcher(app_with_project(dir.path()));
    assert_eq!(app.watched_directories(), [canon(dir.path())]);

    // Opening a directory in the explorer watches it.
    app.handle_input(key(KeyCode::F(1), KeyModifiers::NONE));
    for c in "explorer focus".chars() {
        app.handle_input(key(KeyCode::Char(c), KeyModifiers::NONE));
    }
    app.handle_input(key(KeyCode::Enter, KeyModifiers::NONE));
    app.handle_input(key(KeyCode::Down, KeyModifiers::NONE));
    app.handle_input(key(KeyCode::Right, KeyModifiers::NONE));
    app.sync_watches();
    assert!(
        app.watched_directories()
            .contains(&canon(&dir.path().join("sub")))
    );

    // Opening a file elsewhere watches its directory.
    app.state_mut().workspace_mut().open_path(&file).unwrap();
    app.sync_watches();
    assert!(app.watched_directories().contains(&canon(other.path())));

    // Closing it stops watching.
    app.state_mut().set_focus(crate::app::state::Focus::Editor);
    ctrl(&mut app, 'w');
    app.sync_watches();
    assert!(!app.watched_directories().contains(&canon(other.path())));
}

#[test]
fn a_real_change_on_disk_arrives_as_an_event() {
    let (app, dir, path) = app_with_file("old\n");
    let (mut _app, mut rx) = app_with_watcher(app);

    fs::write(&path, "new\n").unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut seen = false;
    while Instant::now() < deadline && !seen {
        while let Ok(event) = rx.try_recv() {
            if let Event::FilesChanged(paths) = event {
                seen |= paths.iter().any(|changed| changed.ends_with("a.txt"));
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(dir);
    assert!(seen, "no change event for a.txt within 10 seconds");
}

#[test]
fn a_report_of_thousands_of_unrelated_files_still_finds_the_open_one() {
    let (mut app, dir, path) = app_with_file("old\n");
    fs::write(&path, "new\n").unwrap();
    let mut paths: Vec<PathBuf> = (0..5_000)
        .map(|n| dir.path().join(format!("other/branch_file_{n}.rs")))
        .collect();
    paths.push(path);

    app.handle_event(Event::FilesChanged(paths));

    assert_eq!(text(&app), "new\n");
}

#[test]
fn a_report_naming_only_other_files_with_the_same_directory_changes_nothing() {
    let (mut app, dir, path) = app_with_file("old\n");
    fs::write(&path, "new\n").unwrap();

    app.handle_event(Event::FilesChanged(vec![dir.path().join("not_a.txt")]));

    assert_eq!(text(&app), "old\n");
}

#[test]
fn a_report_that_a_watched_directory_was_replaced_starts_watching_the_new_one() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub/old.txt"), "").unwrap();
    let (mut app, mut rx) = app_with_watcher(app_with_project(dir.path()));
    // Open `sub` in the explorer, so it is watched.
    ctrl(&mut app, 'b');
    app.handle_input(key(KeyCode::Down, KeyModifiers::NONE));
    app.handle_input(key(KeyCode::Right, KeyModifiers::NONE));
    app.sync_watches();
    let sub = canon(&dir.path().join("sub"));
    assert!(app.watched_directories().contains(&sub));

    fs::remove_dir_all(&sub).unwrap();
    fs::create_dir(&sub).unwrap();
    app.handle_event(Event::FilesChanged(vec![sub.clone()]));
    app.sync_watches();
    fs::write(sub.join("new.txt"), "").unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut seen = false;
    while Instant::now() < deadline && !seen {
        while let Ok(event) = rx.try_recv() {
            if let Event::FilesChanged(paths) = event {
                seen |= paths.iter().any(|path| path.ends_with("new.txt"));
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(
        seen,
        "no change event for the file made in the new directory"
    );
}
