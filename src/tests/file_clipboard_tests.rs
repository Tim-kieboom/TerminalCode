//! Copy, cut and paste of files and folders in the explorer.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::{AppState, Focus};
use crate::event::action::Action;

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("src/deep")).unwrap();
    fs::create_dir(dir.path().join("other")).unwrap();
    fs::write(dir.path().join("a.txt"), "alpha").unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();
    fs::write(dir.path().join("src/deep/x.rs"), "x").unwrap();
    dir
}

fn app_in(root: &Path) -> App {
    let mut state = AppState::default();
    state.open_project(root).unwrap();
    App::new(state)
}

/// Gives the explorer the keyboard and selects `path`.
fn select(app: &mut App, path: &Path) {
    app.state_mut().set_focus(Focus::Explorer);
    app.state_mut().explorer_mut().reveal(path).unwrap();
    assert_eq!(app.state().explorer().selected_row().unwrap().path, path);
}

fn selected(app: &App) -> PathBuf {
    app.state().explorer().selected_row().unwrap().path.clone()
}

fn told(app: &App) -> String {
    app.state().notifications().texts().join(" | ")
}

fn key(app: &mut App, code: KeyCode) {
    app.handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::CONTROL)));
}

#[test]
fn a_copied_file_pasted_into_a_folder_is_a_copy_there_and_is_selected() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, &dir.path().join("a.txt"));
    app.run_action(Action::FileCopy);

    select(&mut app, &dir.path().join("other"));
    app.run_action(Action::FilePaste);

    assert_eq!(
        fs::read_to_string(dir.path().join("other/a.txt")).unwrap(),
        "alpha"
    );
    assert!(dir.path().join("a.txt").exists());
    assert_eq!(selected(&app), dir.path().join("other/a.txt"));
}

#[test]
fn pasting_into_a_file_goes_into_the_folder_it_is_in() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, &dir.path().join("a.txt"));
    app.run_action(Action::FileCopy);

    select(&mut app, &dir.path().join("src/main.rs"));
    app.run_action(Action::FilePaste);

    assert!(dir.path().join("src/a.txt").exists());
}

#[test]
fn pasting_where_the_name_is_taken_makes_a_numbered_copy_and_replaces_nothing() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, &dir.path().join("a.txt"));
    app.run_action(Action::FileCopy);

    app.run_action(Action::FilePaste);
    app.run_action(Action::FilePaste);

    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "alpha"
    );
    assert!(dir.path().join("a copy.txt").exists());
    assert!(dir.path().join("a copy 2.txt").exists());
}

#[test]
fn a_folder_is_copied_with_what_is_in_it() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, &dir.path().join("src"));
    app.run_action(Action::FileCopy);

    select(&mut app, &dir.path().join("other"));
    app.run_action(Action::FilePaste);

    assert!(dir.path().join("other/src/main.rs").exists());
    assert!(dir.path().join("other/src/deep/x.rs").exists());
}

#[test]
fn a_folder_pasted_into_itself_is_refused_and_nothing_is_made() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, &dir.path().join("src"));
    app.run_action(Action::FileCopy);

    app.run_action(Action::FilePaste);

    assert!(told(&app).contains("itself"), "{}", told(&app));
    assert!(!dir.path().join("src/src").exists());
}

#[test]
fn cut_and_paste_moves_the_file_and_an_open_tab_follows_it() {
    let dir = project();
    let mut app = app_in(dir.path());
    let old = dir.path().join("a.txt");
    app.state_mut().workspace_mut().open_path(&old).unwrap();
    select(&mut app, &old);
    app.run_action(Action::FileCut);

    select(&mut app, &dir.path().join("other"));
    app.run_action(Action::FilePaste);

    let new = dir.path().join("other/a.txt");
    assert!(new.exists() && !old.exists());
    assert_eq!(app.state().editor().buffer().path(), Some(new.as_path()));
    app.run_action(Action::FilePaste);
    assert!(
        told(&app).contains("nothing was copied or cut"),
        "{}",
        told(&app)
    );
}

#[test]
fn cutting_into_the_folder_the_file_is_in_changes_nothing() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, &dir.path().join("a.txt"));
    app.run_action(Action::FileCut);

    app.run_action(Action::FilePaste);

    assert!(dir.path().join("a.txt").exists());
    assert!(
        told(&app).contains("already in that folder"),
        "{}",
        told(&app)
    );
}

#[test]
fn a_cut_over_a_name_that_is_taken_is_refused_and_both_stay() {
    let dir = project();
    fs::write(dir.path().join("other/a.txt"), "mine").unwrap();
    let mut app = app_in(dir.path());
    select(&mut app, &dir.path().join("a.txt"));
    app.run_action(Action::FileCut);

    select(&mut app, &dir.path().join("other"));
    app.run_action(Action::FilePaste);

    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "alpha"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("other/a.txt")).unwrap(),
        "mine"
    );
    assert!(told(&app).contains("already exists"), "{}", told(&app));
}

#[test]
fn the_project_folder_cannot_be_copied_or_cut_but_can_be_pasted_into() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, dir.path());

    app.run_action(Action::FileCopy);
    assert!(
        told(&app).contains("cannot be copied or cut"),
        "{}",
        told(&app)
    );

    select(&mut app, &dir.path().join("src/main.rs"));
    app.run_action(Action::FileCopy);
    select(&mut app, dir.path());
    app.run_action(Action::FilePaste);
    assert!(dir.path().join("main.rs").exists());
}

#[test]
fn a_paste_with_nothing_held_says_so() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, &dir.path().join("a.txt"));

    app.run_action(Action::FilePaste);

    assert!(told(&app).contains("nothing was copied or cut"));
}

#[test]
fn a_copy_of_a_file_that_was_deleted_since_is_an_error_that_stays() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, &dir.path().join("a.txt"));
    app.run_action(Action::FileCopy);
    fs::remove_file(dir.path().join("a.txt")).unwrap();

    select(&mut app, &dir.path().join("other"));
    app.run_action(Action::FilePaste);

    assert!(
        told(&app).contains("could not copy a.txt"),
        "{}",
        told(&app)
    );
    assert!(app.state().notifications().has_errors());
}

#[test]
fn the_keys_in_the_explorer_copy_cut_and_paste_and_leave_the_editors_alone() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, &dir.path().join("a.txt"));

    key(&mut app, KeyCode::Char('c'));
    select(&mut app, &dir.path().join("other"));
    key(&mut app, KeyCode::Char('v'));
    assert!(dir.path().join("other/a.txt").exists());

    select(&mut app, &dir.path().join("src/main.rs"));
    key(&mut app, KeyCode::Char('x'));
    select(&mut app, &dir.path().join("other"));
    key(&mut app, KeyCode::Char('v'));
    assert!(dir.path().join("other/main.rs").exists());
    assert!(!dir.path().join("src/main.rs").exists());
}

#[test]
fn with_an_event_loop_the_copy_runs_on_a_thread_and_ends_with_an_event() {
    let dir = project();
    let (tx, mut events) = mpsc::channel(64);
    let mut app = app_in(dir.path()).with_events(tx);
    select(&mut app, &dir.path().join("src"));
    app.run_action(Action::FileCopy);
    select(&mut app, &dir.path().join("other"));

    app.run_action(Action::FilePaste);

    assert!(told(&app).contains("copying src"), "{}", told(&app));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        while let Ok(event) = events.try_recv() {
            app.handle_event(event);
        }
        if dir.path().join("other/src/deep/x.rs").exists()
            && told(&app).contains("copied src as src")
        {
            break;
        }
        assert!(Instant::now() < deadline, "no end: {}", told(&app));
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(selected(&app), dir.path().join("other/src"));
}
