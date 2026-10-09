use std::fs;
use std::path::Path;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::app::state::AppState;
use crate::entries::{self, EntryError};

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
    fs::create_dir(dir.path().join("docs")).unwrap();
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

fn move_selected_to(app: &mut App, destination: &str) {
    press(app, KeyCode::Char('m'));
    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('u'),
        KeyModifiers::CONTROL,
    )));
    type_str(app, destination);
    press(app, KeyCode::Enter);
}

#[test]
fn m_asks_for_the_path_from_the_project_root() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "src");
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Down);

    press(&mut app, KeyCode::Char('m'));

    let prompt = app.state().name_prompt().unwrap();
    assert_eq!(prompt.name(), Path::new("src/main.rs").to_str().unwrap());
    assert_eq!(prompt.dir(), dir.path());
}

#[test]
fn a_file_moves_into_an_existing_folder() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "a.txt");

    move_selected_to(&mut app, "docs");

    assert!(dir.path().join("docs/a.txt").is_file());
    assert!(!dir.path().join("a.txt").exists());
    let rows = app.state().explorer().rows();
    assert!(rows.iter().any(|row| row.name == "a.txt" && row.depth == 2));
    assert_eq!(app.state().explorer().selected_row().unwrap().name, "a.txt");
}

#[test]
fn a_file_moves_up_out_of_its_folder() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "src");
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Down);

    move_selected_to(&mut app, "main.rs");

    assert!(dir.path().join("main.rs").is_file());
    assert!(!dir.path().join("src/main.rs").exists());
}

#[test]
fn a_move_can_rename_at_the_same_time() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "a.txt");

    move_selected_to(&mut app, "docs/readme.txt");

    assert_eq!(
        fs::read_to_string(dir.path().join("docs/readme.txt")).unwrap(),
        "alpha\n"
    );
}

#[test]
fn an_open_tab_follows_a_move() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "a.txt");
    press(&mut app, KeyCode::Enter);
    app.state_mut()
        .workspace_mut()
        .with_editor(|editor| editor.insert_text("x").unwrap());

    move_selected_to(&mut app, "docs");

    app.state_mut().workspace_mut().save_active().unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("docs/a.txt")).unwrap(),
        "xalpha\n"
    );
    assert!(!dir.path().join("a.txt").exists());
}

#[test]
fn a_folder_moves_with_the_tabs_inside_it() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "src");
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    app.state_mut()
        .workspace_mut()
        .with_editor(|editor| editor.insert_text("x").unwrap());

    select(&mut app, "src");
    move_selected_to(&mut app, "docs");

    app.state_mut().workspace_mut().save_active().unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("docs/src/main.rs")).unwrap(),
        "xfn main() {}\n"
    );
}

#[test]
fn a_move_onto_an_existing_file_is_refused() {
    let dir = project();
    fs::write(dir.path().join("docs/a.txt"), "other").unwrap();
    let mut app = app_in(dir.path());
    select(&mut app, "a.txt");

    move_selected_to(&mut app, "docs");

    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "alpha\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("docs/a.txt")).unwrap(),
        "other"
    );
    assert!(app.state().name_prompt().is_some());
    assert!(app.state().notifications().has_errors());
}

#[test]
fn a_folder_cannot_move_into_itself_and_leaves_no_folders_behind() {
    let dir = project();

    let error = entries::move_to(&dir.path().join("src"), dir.path(), "src/new/src").unwrap_err();

    assert!(matches!(error, EntryError::IntoItself(_)));
    assert!(!dir.path().join("src/new").exists());
}

#[test]
fn a_move_outside_the_project_is_refused() {
    let dir = project();

    let error = entries::move_to(&dir.path().join("a.txt"), dir.path(), "../a.txt").unwrap_err();

    assert!(matches!(error, EntryError::Outside(_)));
    assert!(dir.path().join("a.txt").exists());
}

#[test]
fn moving_to_the_place_it_already_is_changes_nothing() {
    let dir = project();

    let to = entries::move_to(&dir.path().join("src"), dir.path(), "src").unwrap();

    assert_eq!(to, dir.path().join("src"));
    assert!(dir.path().join("src/main.rs").exists());
}

#[test]
fn the_project_folder_cannot_be_moved() {
    let dir = project();
    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Home);

    press(&mut app, KeyCode::Char('m'));

    assert!(app.state().name_prompt().is_none());
    assert!(!app.state().notifications().texts().is_empty());
}
