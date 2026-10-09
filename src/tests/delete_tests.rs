use std::fs;
use std::path::Path;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::app::state::AppState;
use crate::removal;

fn press(app: &mut App, code: KeyCode) {
    app.handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

/// The empty document every new app starts with.
const UNTITLED: usize = 1;

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(dir.path().join("a.txt"), "alpha\n").unwrap();
    dir
}

/// Stands in for the OS trash (which tests must not fill): removes the file.
fn trash_that_works(path: &Path) -> Result<(), String> {
    removal::remove_permanently(path).map_err(|error| error.to_string())
}

fn trash_that_fails(_: &Path) -> Result<(), String> {
    Err("no trash on this mount".to_owned())
}

fn app_in(root: &Path, trash: removal::TrashFn) -> App {
    let mut state = AppState::default();
    state.open_project(root).unwrap();
    let mut app = App::new(state).with_trash(trash);
    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('b'),
        KeyModifiers::CONTROL,
    )));
    app
}

/// Moves the explorer's selection onto the row called `name`.
fn select(app: &mut App, name: &str) {
    press(app, KeyCode::Home);
    for _ in 0..20 {
        if app.state().explorer().selected_row().unwrap().name == name {
            return;
        }
        press(app, KeyCode::Down);
    }
    panic!("no row called {name}");
}

fn row_names(app: &App) -> Vec<String> {
    let rows = app.state().explorer().rows();
    rows.iter().map(|row| row.name.clone()).collect()
}

#[test]
fn delete_moves_the_selected_file_to_the_trash() {
    let dir = project();
    let mut app = app_in(dir.path(), trash_that_works);

    select(&mut app, "a.txt");
    press(&mut app, KeyCode::Delete);

    assert!(!dir.path().join("a.txt").exists());
    assert!(!row_names(&app).contains(&"a.txt".to_owned()));
    assert!(app.state().confirm().is_none());
    assert_eq!(
        app.state().notifications().texts(),
        ["moved a.txt to the trash"]
    );
}

#[test]
fn delete_takes_a_folder_with_its_contents() {
    let dir = project();
    let mut app = app_in(dir.path(), trash_that_works);

    select(&mut app, "src");
    press(&mut app, KeyCode::Delete);

    assert!(!dir.path().join("src").exists());
    assert!(!row_names(&app).contains(&"src".to_owned()));
}

#[test]
fn the_project_folder_itself_cannot_be_deleted() {
    let dir = project();
    let mut app = app_in(dir.path(), trash_that_works);

    press(&mut app, KeyCode::Delete);

    assert!(dir.path().join("a.txt").exists());
    assert!(app.state().confirm().is_none());
    assert!(!app.state().notifications().texts().is_empty());
}

#[test]
fn a_failed_trash_offers_permanent_delete_and_enter_cancels() {
    let dir = project();
    let mut app = app_in(dir.path(), trash_that_fails);

    select(&mut app, "a.txt");
    press(&mut app, KeyCode::Delete);

    let text = app.state().confirm().unwrap().text();
    assert!(text.contains("a.txt could not be moved to the trash"));
    assert!(text.contains("no trash on this mount"));
    assert!(text.contains("permanently"));

    press(&mut app, KeyCode::Enter);

    assert!(app.state().confirm().is_none());
    assert!(dir.path().join("a.txt").exists());
}

#[test]
fn yes_deletes_permanently_after_a_failed_trash() {
    let dir = project();
    let mut app = app_in(dir.path(), trash_that_fails);

    select(&mut app, "a.txt");
    press(&mut app, KeyCode::Delete);
    press(&mut app, KeyCode::Char('y'));

    assert!(!dir.path().join("a.txt").exists());
    assert!(app.state().confirm().is_none());
    assert!(!row_names(&app).contains(&"a.txt".to_owned()));
}

#[test]
fn any_other_key_cancels_the_question() {
    let dir = project();
    let mut app = app_in(dir.path(), trash_that_fails);

    select(&mut app, "a.txt");
    press(&mut app, KeyCode::Delete);
    press(&mut app, KeyCode::Char('n'));

    assert!(app.state().confirm().is_none());
    assert!(dir.path().join("a.txt").exists());
}

#[test]
fn deleting_closes_the_tab_of_the_file() {
    let dir = project();
    let mut app = app_in(dir.path(), trash_that_works);
    select(&mut app, "a.txt");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.state().workspace().document_count(), UNTITLED + 1);

    press(&mut app, KeyCode::Delete);

    assert_eq!(app.state().workspace().document_count(), UNTITLED);
}

#[test]
fn deleting_a_folder_closes_the_tabs_inside_it() {
    let dir = project();
    let mut app = app_in(dir.path(), trash_that_works);
    select(&mut app, "src");
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.state().workspace().document_count(), UNTITLED + 1);

    select(&mut app, "src");
    press(&mut app, KeyCode::Delete);

    assert_eq!(app.state().workspace().document_count(), UNTITLED);
    assert!(!dir.path().join("src").exists());
}

#[test]
fn unsaved_changes_are_asked_about_first() {
    let dir = project();
    let mut app = app_in(dir.path(), trash_that_works);
    select(&mut app, "a.txt");
    press(&mut app, KeyCode::Enter);
    app.state_mut()
        .workspace_mut()
        .with_editor(|editor| editor.insert_text("x").unwrap());
    select(&mut app, "a.txt");

    press(&mut app, KeyCode::Delete);

    let text = app.state().confirm().unwrap().text();
    assert!(text.contains("unsaved changes"));
    assert!(text.contains("a.txt"));
    assert!(dir.path().join("a.txt").exists());

    press(&mut app, KeyCode::Enter);
    assert!(dir.path().join("a.txt").exists());
    assert_eq!(app.state().workspace().document_count(), UNTITLED + 1);

    press(&mut app, KeyCode::Delete);
    press(&mut app, KeyCode::Char('y'));

    assert!(!dir.path().join("a.txt").exists());
    assert_eq!(app.state().workspace().document_count(), UNTITLED);
}

#[test]
fn the_trash_question_follows_the_unsaved_question() {
    let dir = project();
    let mut app = app_in(dir.path(), trash_that_fails);
    select(&mut app, "a.txt");
    press(&mut app, KeyCode::Enter);
    app.state_mut()
        .workspace_mut()
        .with_editor(|editor| editor.insert_text("x").unwrap());
    select(&mut app, "a.txt");

    press(&mut app, KeyCode::Delete);
    press(&mut app, KeyCode::Char('y'));

    assert!(app.state().confirm().unwrap().text().contains("trash"));
    assert!(dir.path().join("a.txt").exists());
    assert_eq!(app.state().workspace().document_count(), UNTITLED + 1);
}

#[test]
fn a_file_that_is_already_gone_gives_an_error_not_a_crash() {
    let dir = project();
    let mut app = app_in(dir.path(), trash_that_fails);
    select(&mut app, "a.txt");
    press(&mut app, KeyCode::Delete);
    fs::remove_file(dir.path().join("a.txt")).unwrap();

    press(&mut app, KeyCode::Char('y'));

    assert!(app.state().notifications().has_errors());
}
