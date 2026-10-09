use std::fs;
use std::path::Path;

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
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

fn mouse(app: &mut App, kind: MouseEventKind, column: u16, row: u16) {
    app.handle_input(InputEvent::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

fn right_click(app: &mut App, (column, row): (u16, u16)) {
    mouse(app, MouseEventKind::Down(MouseButton::Right), column, row);
}

fn left_click(app: &mut App, (column, row): (u16, u16)) {
    mouse(app, MouseEventKind::Down(MouseButton::Left), column, row);
}

/// The screen cell of the first character of `text`, as last drawn.
fn cell_of(terminal: &Terminal<TestBackend>, text: &str) -> (u16, u16) {
    let screen = terminal.backend().to_string();
    for (row, line) in screen.lines().enumerate() {
        if let Some(byte) = line.find(text) {
            return (line[..byte].chars().count() as u16, row as u16);
        }
    }
    panic!("{text} is not on screen:\n{screen}");
}

fn target_name(app: &App) -> String {
    let menu = app.state().menu().unwrap();
    menu.target().file_name().unwrap().to_string_lossy().into()
}

#[test]
fn a_right_click_selects_the_row_and_opens_the_menu_on_it() {
    let dir = project();
    let (mut app, mut terminal) = app_in(dir.path());
    select(&mut app, "a.txt");
    draw(&mut app, &mut terminal);

    right_click(&mut app, cell_of(&terminal, "b.txt"));

    assert_eq!(target_name(&app), "b.txt");
    assert_eq!(app.state().explorer().selected_row().unwrap().name, "b.txt");
    assert_eq!(labels(&app).len(), 5);
}

#[test]
fn a_right_click_on_empty_space_opens_the_menu_on_the_project_folder() {
    let dir = project();
    let (mut app, mut terminal) = app_in(dir.path());
    let body = app.state().explorer().body_for_tests();

    right_click(&mut app, (body.x + 2, body.bottom() - 2));
    draw(&mut app, &mut terminal);

    assert_eq!(app.state().menu().unwrap().target(), dir.path());
    assert_eq!(labels(&app), ["New File", "New Folder"]);
}

#[test]
fn clicking_an_item_runs_it_on_the_row_the_menu_was_opened_on() {
    let dir = project();
    let (mut app, mut terminal) = app_in(dir.path());
    select(&mut app, "a.txt");
    draw(&mut app, &mut terminal);
    right_click(&mut app, cell_of(&terminal, "b.txt"));
    draw(&mut app, &mut terminal);

    left_click(&mut app, cell_of(&terminal, "Rename"));

    assert!(app.state().menu().is_none());
    assert_eq!(app.state().name_prompt().unwrap().name(), "b.txt");
}

#[test]
fn clicking_delete_deletes_the_clicked_row() {
    let dir = project();
    let (mut app, mut terminal) = app_in(dir.path());
    draw(&mut app, &mut terminal);
    right_click(&mut app, cell_of(&terminal, "b.txt"));
    draw(&mut app, &mut terminal);

    left_click(&mut app, cell_of(&terminal, "Delete"));

    assert!(!dir.path().join("b.txt").exists());
    assert!(dir.path().join("a.txt").exists());
}

#[test]
fn clicking_the_frame_or_the_separator_keeps_the_menu_open() {
    let dir = project();
    let (mut app, mut terminal) = app_in(dir.path());
    draw(&mut app, &mut terminal);
    right_click(&mut app, cell_of(&terminal, "a.txt"));
    draw(&mut app, &mut terminal);
    let (x, top) = cell_of(&terminal, "New File");

    left_click(&mut app, (x, top - 1));
    left_click(&mut app, (x, top + 3));

    assert!(app.state().menu().is_some());
    assert!(app.state().name_prompt().is_none());
    assert!(dir.path().join("a.txt").exists());
}

#[test]
fn a_left_click_outside_closes_the_menu_and_acts_on_what_it_hit() {
    let dir = project();
    let (mut app, mut terminal) = app_in(dir.path());
    draw(&mut app, &mut terminal);
    right_click(&mut app, cell_of(&terminal, "a.txt"));
    draw(&mut app, &mut terminal);
    // The menu hangs over the rows below a.txt; the editor side is free.
    let editor = (terminal.backend().buffer().area.width - 3, 3);

    left_click(&mut app, editor);

    assert!(app.state().menu().is_none());
    assert_eq!(app.state().focus(), crate::app::state::Focus::Editor);
}

#[test]
fn a_right_click_outside_reopens_the_menu_on_the_new_row() {
    let dir = project();
    let (mut app, mut terminal) = app_in(dir.path());
    draw(&mut app, &mut terminal);
    let src = cell_of(&terminal, "src");
    right_click(&mut app, cell_of(&terminal, "a.txt"));
    draw(&mut app, &mut terminal);

    right_click(&mut app, src);

    assert_eq!(target_name(&app), "src");
}

#[test]
fn the_wheel_closes_the_menu_and_touches_nothing_else() {
    let dir = project();
    let (mut app, mut terminal) = app_in(dir.path());
    draw(&mut app, &mut terminal);
    right_click(&mut app, cell_of(&terminal, "a.txt"));
    draw(&mut app, &mut terminal);

    mouse(&mut app, MouseEventKind::ScrollDown, 2, 2);

    assert!(app.state().menu().is_none());
    assert!(dir.path().join("a.txt").exists());
}

#[test]
fn releasing_the_button_that_opened_the_menu_does_not_close_it() {
    let dir = project();
    let (mut app, mut terminal) = app_in(dir.path());
    draw(&mut app, &mut terminal);
    let (column, row) = cell_of(&terminal, "a.txt");
    right_click(&mut app, (column, row));
    draw(&mut app, &mut terminal);

    mouse(
        &mut app,
        MouseEventKind::Up(MouseButton::Right),
        column,
        row,
    );

    assert!(app.state().menu().is_some());
}

#[test]
fn a_left_click_on_another_row_closes_the_menu_and_selects_that_row() {
    let dir = project();
    let (mut app, mut terminal) = app_in(dir.path());
    draw(&mut app, &mut terminal);
    let src = cell_of(&terminal, "src");
    right_click(&mut app, cell_of(&terminal, "a.txt"));
    draw(&mut app, &mut terminal);

    left_click(&mut app, src);

    assert!(app.state().menu().is_none());
    assert_eq!(app.state().explorer().selected_row().unwrap().name, "src");
}

/// The text of the screen line that holds `label`.
fn line_with(terminal: &Terminal<TestBackend>, label: &str) -> String {
    let screen = terminal.backend().to_string();
    screen
        .lines()
        .find(|line| line.contains(label))
        .unwrap_or_else(|| panic!("{label} is not on screen:\n{screen}"))
        .to_owned()
}

#[test]
fn each_item_shows_the_keys_bound_to_its_action() {
    let dir = project();
    let (mut app, mut terminal) = app_in(dir.path());
    select(&mut app, "a.txt");
    open_menu(&mut app);

    draw(&mut app, &mut terminal);

    assert!(line_with(&terminal, "New File").contains(" a "));
    assert!(line_with(&terminal, "New Folder").contains(" A "));
    assert!(
        line_with(&terminal, "Rename").contains(" r "),
        "shortest wins over f2"
    );
    assert!(line_with(&terminal, "Delete").contains("delete"));
}

#[test]
fn the_keys_sit_at_the_right_edge_of_the_menu() {
    let dir = project();
    let (mut app, mut terminal) = app_in(dir.path());
    select(&mut app, "a.txt");
    open_menu(&mut app);

    draw(&mut app, &mut terminal);

    let line = line_with(&terminal, "New Folder");
    let keys = line.rfind(" A ").unwrap();
    let after = &line[keys + 3..];
    assert!(after.starts_with("│"), "{line}");
}

#[test]
fn a_rebound_key_shows_in_the_menu_and_an_unbound_action_shows_none() {
    let dir = project();
    let keymap = crate::keymap::Keymap::from_toml(
        r#"
[[binding]]
keys = "ctrl+b"
action = "focus_explorer"

[[binding]]
keys = "f9"
context = "explorer"
action = "context_menu"

[[binding]]
keys = "f6"
context = "explorer"
action = "rename"

[[binding]]
keys = "home"
context = "explorer"
action = { explorer = "first" }

[[binding]]
keys = "down"
context = "explorer"
action = { explorer = "down" }
"#,
    )
    .unwrap();
    let mut state = AppState::default();
    state.open_project(dir.path()).unwrap();
    let mut app = App::with_keymap(state, keymap);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('b'),
        KeyModifiers::CONTROL,
    )));
    select(&mut app, "a.txt");
    press(&mut app, KeyCode::F(9));

    draw(&mut app, &mut terminal);

    assert!(line_with(&terminal, "Rename").contains(" f6 "));
    assert!(!line_with(&terminal, "Delete").contains("delete"));
}
