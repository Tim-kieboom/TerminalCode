use std::fs;
use std::path::{Path, PathBuf};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::AppState;
use crate::entries::{self, EntryError};
use crate::event::Event;
use crate::watcher::FsWatcher;

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
    fs::write(dir.path().join("b.txt"), "beta\n").unwrap();
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

/// Renames the selected row: `r`, replace the name, Enter.
fn rename_selected_to(app: &mut App, name: &str) {
    press(app, KeyCode::Char('r'));
    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('u'),
        KeyModifiers::CONTROL,
    )));
    type_str(app, name);
    press(app, KeyCode::Enter);
}

fn tab_names(app: &App) -> Vec<String> {
    app.state().workspace().tab_names().0
}

#[test]
fn r_asks_for_a_new_name_starting_from_the_old_one() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "a.txt");

    press(&mut app, KeyCode::Char('r'));

    assert_eq!(app.state().name_prompt().unwrap().name(), "a.txt");
}

#[test]
fn renaming_a_file_moves_it_and_selects_it_under_the_new_name() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "a.txt");

    rename_selected_to(&mut app, "c.txt");

    assert!(!dir.path().join("a.txt").exists());
    assert_eq!(
        fs::read_to_string(dir.path().join("c.txt")).unwrap(),
        "alpha\n"
    );
    assert!(app.state().name_prompt().is_none());
    let names = row_names(&app);
    assert!(names.contains(&"c.txt".to_owned()));
    assert!(!names.contains(&"a.txt".to_owned()));
    assert_eq!(app.state().explorer().selected_row().unwrap().name, "c.txt");
}

#[test]
fn an_open_tab_follows_the_rename_with_its_unsaved_text() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "a.txt");
    press(&mut app, KeyCode::Enter);
    app.state_mut()
        .workspace_mut()
        .with_editor(|editor| editor.insert_text("x").unwrap());

    rename_selected_to(&mut app, "c.txt");

    assert!(tab_names(&app).iter().any(|name| name.contains("c.txt")));
    assert!(!tab_names(&app).iter().any(|name| name.contains("a.txt")));
    assert_eq!(app.state().workspace().dirty_documents().len(), 1);
    assert_eq!(app.state().editor().buffer().text(), "xalpha\n");

    app.state_mut().workspace_mut().save_active().unwrap();

    assert_eq!(
        fs::read_to_string(dir.path().join("c.txt")).unwrap(),
        "xalpha\n"
    );
    assert!(!dir.path().join("a.txt").exists());
}

#[test]
fn renaming_a_folder_moves_the_tabs_inside_it() {
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
    rename_selected_to(&mut app, "source");

    assert!(dir.path().join("source/main.rs").is_file());
    app.state_mut().workspace_mut().save_active().unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("source/main.rs")).unwrap(),
        "xfn main() {}\n"
    );
    assert!(!dir.path().join("src").exists());
}

#[test]
fn a_renamed_folder_stays_open_in_the_tree() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "src");
    press(&mut app, KeyCode::Right);
    assert!(row_names(&app).contains(&"main.rs".to_owned()));

    select(&mut app, "src");
    rename_selected_to(&mut app, "source");

    let names = row_names(&app);
    assert!(names.contains(&"source".to_owned()));
    assert!(names.contains(&"main.rs".to_owned()));
}

#[test]
fn a_name_that_exists_is_refused_and_nothing_is_overwritten() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "a.txt");

    rename_selected_to(&mut app, "b.txt");

    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "alpha\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("b.txt")).unwrap(),
        "beta\n"
    );
    assert!(app.state().name_prompt().is_some());
    assert!(app.state().notifications().has_errors());
}

#[test]
fn escape_and_an_unchanged_name_change_nothing() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "a.txt");

    press(&mut app, KeyCode::Char('r'));
    type_str(&mut app, "zzz");
    press(&mut app, KeyCode::Esc);
    assert!(dir.path().join("a.txt").exists());

    press(&mut app, KeyCode::F(2));
    press(&mut app, KeyCode::Enter);
    assert!(dir.path().join("a.txt").exists());
    assert!(app.state().name_prompt().is_none());
}

#[test]
fn the_project_folder_cannot_be_renamed() {
    let dir = project();
    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::Home);

    press(&mut app, KeyCode::Char('r'));

    assert!(app.state().name_prompt().is_none());
    assert!(!app.state().notifications().texts().is_empty());
}

#[test]
fn a_late_event_for_the_old_path_is_harmless() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "a.txt");
    press(&mut app, KeyCode::Enter);
    app.state_mut()
        .workspace_mut()
        .with_editor(|editor| editor.insert_text("x").unwrap());
    let old = dir.path().join("a.txt");

    rename_selected_to(&mut app, "c.txt");
    app.handle_event(Event::FilesChanged(vec![old, dir.path().to_path_buf()]));

    assert!(!app.state().notifications().has_errors());
    assert_eq!(app.state().editor().buffer().text(), "xalpha\n");
    assert_eq!(app.state().workspace().dirty_documents().len(), 1);
}

#[test]
fn a_late_event_for_a_file_in_the_old_folder_is_harmless() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "src");
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    let old = dir.path().join("src/main.rs");

    select(&mut app, "src");
    rename_selected_to(&mut app, "source");
    app.handle_event(Event::FilesChanged(vec![old, dir.path().join("src")]));

    assert!(!app.state().notifications().has_errors());
    assert!(app.state().notifications().texts().is_empty());
}

#[test]
fn a_real_deletion_of_the_new_path_is_still_reported() {
    let dir = project();
    let mut app = app_in(dir.path());
    select(&mut app, "a.txt");
    press(&mut app, KeyCode::Enter);

    rename_selected_to(&mut app, "c.txt");
    fs::remove_file(dir.path().join("c.txt")).unwrap();
    app.handle_event(Event::FilesChanged(vec![dir.path().join("c.txt")]));

    assert!(!app.state().notifications().texts().is_empty());
}

#[test]
fn the_watcher_follows_a_renamed_folder_with_an_open_file() {
    let dir = project();
    let root = fs::canonicalize(dir.path()).unwrap();
    let (tx, _rx) = mpsc::channel::<Event>(256);
    let mut app = app_in(&root).with_watcher(FsWatcher::new(tx).unwrap());
    select(&mut app, "src");
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    app.sync_watches();
    assert!(app.watched_directories().contains(&root.join("src")));

    select(&mut app, "src");
    rename_selected_to(&mut app, "source");
    app.sync_watches();

    let watched: Vec<PathBuf> = app.watched_directories();
    assert!(watched.contains(&root.join("source")));
    assert!(!watched.contains(&root.join("src")));
}

#[test]
fn renaming_never_replaces_an_entry() {
    let dir = project();

    let error = entries::rename(&dir.path().join("a.txt"), "b.txt").unwrap_err();

    assert!(matches!(error, EntryError::Exists(_)));
    assert_eq!(
        fs::read_to_string(dir.path().join("b.txt")).unwrap(),
        "beta\n"
    );
    assert!(dir.path().join("a.txt").exists());
}

#[test]
fn renaming_to_a_name_outside_the_folder_is_refused() {
    let dir = project();
    for name in ["../x", "/tmp/x", ""] {
        let error = entries::rename(&dir.path().join("a.txt"), name).unwrap_err();
        assert!(
            matches!(error, EntryError::Outside(_) | EntryError::Empty),
            "{name}"
        );
    }
    assert!(dir.path().join("a.txt").exists());
}

#[test]
fn a_name_with_folders_moves_the_file_into_them() {
    let dir = project();

    let to = entries::rename(&dir.path().join("a.txt"), "docs/a.txt").unwrap();

    assert_eq!(to, dir.path().join("docs/a.txt"));
    assert!(to.is_file());
    assert!(!dir.path().join("a.txt").exists());
}
