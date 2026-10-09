use std::fs;
use std::path::Path;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::app::state::{AppState, Focus};
use crate::entries::{self, EntryError, EntryKind};

fn press(app: &mut App, code: KeyCode) {
    app.handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(dir.path().join("a.txt"), "alpha\n").unwrap();
    dir
}

fn app_in(root: &Path) -> App {
    let mut state = AppState::default();
    state.open_project(root).unwrap();
    let mut app = App::new(state);
    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('b'),
        KeyModifiers::CONTROL,
    )));
    app
}

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
fn a_makes_a_file_in_the_selected_folder_and_opens_it() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "src");

    press(&mut app, KeyCode::Char('a'));
    assert_eq!(
        app.state().name_prompt().unwrap().dir(),
        dir.path().join("src")
    );
    type_str(&mut app, "lib.rs");
    press(&mut app, KeyCode::Enter);

    assert!(dir.path().join("src/lib.rs").is_file());
    assert!(app.state().name_prompt().is_none());
    assert!(row_names(&app).contains(&"lib.rs".to_owned()));
    assert_eq!(app.state().focus(), Focus::Editor);
    assert_eq!(
        app.state().explorer().selected_row().unwrap().name,
        "lib.rs"
    );
}

#[test]
fn a_file_selected_in_the_explorer_makes_the_new_file_beside_it() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "a.txt");

    press(&mut app, KeyCode::Char('a'));
    type_str(&mut app, "b.txt");
    press(&mut app, KeyCode::Enter);

    assert!(dir.path().join("b.txt").is_file());
}

#[test]
fn shift_a_makes_a_folder_and_stays_in_the_explorer() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "a.txt");

    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('A'),
        KeyModifiers::SHIFT,
    )));
    type_str(&mut app, "docs");
    press(&mut app, KeyCode::Enter);

    assert!(dir.path().join("docs").is_dir());
    assert_eq!(app.state().focus(), Focus::Explorer);
    assert!(row_names(&app).contains(&"docs".to_owned()));
}

#[test]
fn a_name_with_folders_makes_them_and_reveals_the_file() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "a.txt");

    press(&mut app, KeyCode::Char('a'));
    type_str(&mut app, "deep/er/file.rs");
    press(&mut app, KeyCode::Enter);

    assert!(dir.path().join("deep/er/file.rs").is_file());
    let names = row_names(&app);
    assert!(names.contains(&"deep".to_owned()));
    assert!(names.contains(&"er".to_owned()));
    assert!(names.contains(&"file.rs".to_owned()));
}

#[test]
fn a_name_that_exists_is_an_error_and_the_prompt_stays_open() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "a.txt");

    press(&mut app, KeyCode::Char('a'));
    type_str(&mut app, "a.txt");
    press(&mut app, KeyCode::Enter);

    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "alpha\n"
    );
    assert_eq!(app.state().name_prompt().unwrap().name(), "a.txt");
    assert!(app.state().notifications().has_errors());
}

#[test]
fn escape_cancels_and_makes_nothing() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "src");

    press(&mut app, KeyCode::Char('a'));
    type_str(&mut app, "x.rs");
    press(&mut app, KeyCode::Esc);

    assert!(app.state().name_prompt().is_none());
    assert!(!dir.path().join("src/x.rs").exists());
}

#[test]
fn an_empty_name_is_an_error_and_the_prompt_stays_open() {
    let dir = project();
    let mut app = app_in(dir.path());

    press(&mut app, KeyCode::Char('a'));
    press(&mut app, KeyCode::Enter);

    assert!(app.state().name_prompt().is_some());
    assert!(app.state().notifications().has_errors());
}

#[test]
fn creating_never_overwrites_an_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("keep.txt"), "mine").unwrap();

    let error = entries::create(dir.path(), "keep.txt", EntryKind::File).unwrap_err();

    assert!(matches!(error, EntryError::Exists(_)));
    assert_eq!(
        fs::read_to_string(dir.path().join("keep.txt")).unwrap(),
        "mine"
    );
}

#[test]
fn creating_an_existing_folder_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();

    let error = entries::create(dir.path(), "src", EntryKind::Folder).unwrap_err();

    assert!(matches!(error, EntryError::Exists(_)));
}

#[test]
fn names_that_leave_the_folder_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["../x", "/etc/x", "a/../../x"] {
        let error = entries::create(dir.path(), name, EntryKind::File).unwrap_err();
        assert!(matches!(error, EntryError::Outside(_)), "{name}");
    }
}

#[test]
fn a_file_in_the_way_of_a_folder_name_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a"), "").unwrap();

    let error = entries::create(dir.path(), "a/b.rs", EntryKind::File).unwrap_err();

    assert!(matches!(error, EntryError::Io { .. }));
}
