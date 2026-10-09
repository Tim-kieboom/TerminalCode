use std::fs;
use std::path::Path;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::app::state::AppState;
use crate::components::menu::Entry;
use crate::components::prepare_and_render;
use crate::removal;

fn press(app: &mut App, code: KeyCode) {
    app.handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn open_menu(app: &mut App) {
    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::F(10),
        KeyModifiers::SHIFT,
    )));
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(dir.path().join("a.txt"), "alpha\n").unwrap();
    fs::write(dir.path().join("b.txt"), "beta\n").unwrap();
    dir
}

/// Stands in for the OS trash, which tests must not fill.
fn trash_that_works(path: &Path) -> Result<(), String> {
    removal::remove_permanently(path).map_err(|error| error.to_string())
}

/// An app with the explorer focused, drawn once so it knows where its rows are.
fn app_in(root: &Path) -> (App, Terminal<TestBackend>) {
    let mut state = AppState::default();
    state.open_project(root).unwrap();
    let mut app = App::new(state).with_trash(trash_that_works);
    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('b'),
        KeyModifiers::CONTROL,
    )));
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    draw(&mut app, &mut terminal);
    (app, terminal)
}

fn draw(app: &mut App, terminal: &mut Terminal<TestBackend>) {
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
}

fn select(app: &mut App, name: &str) {
    press(app, KeyCode::Home);
    for _ in 0..50 {
        if app.state().explorer().selected_row().unwrap().name == name {
            return;
        }
        press(app, KeyCode::Down);
    }
    panic!("no row called {name}");
}

fn labels(app: &App) -> Vec<&'static str> {
    app.state()
        .menu()
        .unwrap()
        .entries()
        .iter()
        .map(|entry| match entry {
            Entry::Item { label, .. } => *label,
            Entry::Separator => "---",
        })
        .collect()
}

fn selected_label(app: &App) -> &'static str {
    let menu = app.state().menu().unwrap();
    match &menu.entries()[menu.selected()] {
        Entry::Item { label, .. } => label,
        Entry::Separator => "---",
    }
}

#[test]
fn shift_f10_opens_a_menu_on_the_selected_row() {
    let dir = project();
    let (mut app, _terminal) = app_in(dir.path());
    select(&mut app, "a.txt");

    open_menu(&mut app);

    let menu = app.state().menu().unwrap();
    assert_eq!(menu.target(), dir.path().join("a.txt"));
    assert_eq!(
        labels(&app),
        ["New File", "New Folder", "Rename", "---", "Delete"]
    );
}

#[test]
fn the_project_folder_only_offers_to_add_things() {
    let dir = project();
    let (mut app, _terminal) = app_in(dir.path());
    select(&mut app, &app_root_name(dir.path()));

    open_menu(&mut app);

    assert_eq!(labels(&app), ["New File", "New Folder"]);
}

fn app_root_name(root: &Path) -> String {
    root.file_name().unwrap().to_string_lossy().into_owned()
}

#[test]
fn the_selection_skips_the_separator_and_stops_at_both_ends() {
    let dir = project();
    let (mut app, _terminal) = app_in(dir.path());
    select(&mut app, "a.txt");
    open_menu(&mut app);

    press(&mut app, KeyCode::Up);
    assert_eq!(selected_label(&app), "New File");
    for _ in 0..3 {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(selected_label(&app), "Delete");
    press(&mut app, KeyCode::Down);
    assert_eq!(selected_label(&app), "Delete");
}

#[test]
fn the_menu_takes_the_keys_and_escape_closes_it_without_doing_anything() {
    let dir = project();
    let (mut app, _terminal) = app_in(dir.path());
    select(&mut app, "a.txt");
    open_menu(&mut app);

    press(&mut app, KeyCode::Char('r'));
    assert!(app.state().menu().is_some());
    assert!(app.state().name_prompt().is_none());

    press(&mut app, KeyCode::Esc);
    assert!(app.state().menu().is_none());
    assert!(dir.path().join("a.txt").exists());
}

#[test]
fn enter_on_new_file_asks_for_a_name_beside_the_row() {
    let dir = project();
    let (mut app, _terminal) = app_in(dir.path());
    select(&mut app, "a.txt");
    open_menu(&mut app);

    press(&mut app, KeyCode::Enter);

    assert!(app.state().menu().is_none());
    let prompt = app.state().name_prompt().unwrap();
    assert_eq!(prompt.dir(), dir.path());
}

#[test]
fn enter_on_rename_starts_from_the_old_name() {
    let dir = project();
    let (mut app, _terminal) = app_in(dir.path());
    select(&mut app, "b.txt");
    open_menu(&mut app);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);

    press(&mut app, KeyCode::Enter);

    assert_eq!(app.state().name_prompt().unwrap().name(), "b.txt");
}

#[test]
fn enter_on_delete_deletes_the_row_the_menu_was_opened_on() {
    let dir = project();
    let (mut app, _terminal) = app_in(dir.path());
    select(&mut app, "a.txt");
    open_menu(&mut app);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!(selected_label(&app), "Delete");

    press(&mut app, KeyCode::Enter);

    assert!(!dir.path().join("a.txt").exists());
    assert!(dir.path().join("b.txt").exists());
}

#[test]
fn a_path_that_vanished_is_reported_and_nothing_runs() {
    let dir = project();
    let (mut app, _terminal) = app_in(dir.path());
    select(&mut app, "a.txt");
    open_menu(&mut app);
    fs::remove_file(dir.path().join("a.txt")).unwrap();
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);

    press(&mut app, KeyCode::Enter);

    assert!(app.state().menu().is_none());
    assert!(app.state().name_prompt().is_none());
    assert!(dir.path().join("b.txt").exists(), "b.txt was not touched");
    let note = app.state().latest_notification().unwrap();
    assert!(note.contains("no longer exists"), "{note}");
}

#[test]
fn a_new_file_is_not_made_in_a_folder_that_vanished() {
    let dir = project();
    let (mut app, _terminal) = app_in(dir.path());
    select(&mut app, "src");
    open_menu(&mut app);
    fs::remove_dir_all(dir.path().join("src")).unwrap();

    press(&mut app, KeyCode::Enter);

    assert!(app.state().name_prompt().is_none());
    assert!(!dir.path().join("src").exists());
}

#[test]
fn the_menu_is_drawn_under_the_selected_row() {
    let dir = project();
    let (mut app, mut terminal) = app_in(dir.path());
    select(&mut app, "a.txt");
    open_menu(&mut app);

    draw(&mut app, &mut terminal);

    let screen = terminal.backend().to_string();
    for label in ["New File", "New Folder", "Rename", "Delete"] {
        assert!(screen.contains(label), "{screen}");
    }
}

#[test]
fn a_menu_near_the_bottom_opens_above_its_row() {
    let dir = tempfile::tempdir().unwrap();
    for n in 0..30 {
        fs::write(dir.path().join(format!("f{n:02}.txt")), "").unwrap();
    }
    let (mut app, mut terminal) = app_in(dir.path());
    select(&mut app, "f20.txt");
    draw(&mut app, &mut terminal);
    open_menu(&mut app);

    draw(&mut app, &mut terminal);

    let screen = terminal.backend().to_string();
    let line_of = |text: &str| screen.lines().position(|line| line.contains(text));
    let row = line_of("f20.txt").expect("the selected row is on screen");
    let menu = line_of("New File").expect("the menu is on screen");
    assert!(
        menu < row,
        "menu on line {menu}, row on line {row}\n{screen}"
    );
    assert!(line_of("Delete").is_some(), "{screen}");
}
