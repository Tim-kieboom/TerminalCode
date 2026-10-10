use std::fs;
use std::path::Path;

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::app::state::{AppState, Focus};
use crate::components::ComponentKind;
use crate::components::prepare_and_render;
use crate::ui::layout::LayoutTree;

fn key(code: KeyCode, modifiers: KeyModifiers) -> InputEvent {
    InputEvent::Key(KeyEvent::new(code, modifiers))
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_input(key(code, KeyModifiers::NONE));
}

fn ctrl(app: &mut App, c: char) {
    app.handle_input(key(KeyCode::Char(c), KeyModifiers::CONTROL));
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn click(column: u16, row: u16) -> InputEvent {
    InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
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
    App::new(state)
}

/// Draws one frame so the components know their screen areas.
fn draw(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    terminal.backend().to_string()
}

fn select(app: &mut App, name: &str) {
    press(app, KeyCode::Home);
    while app.state().explorer().selected_row().unwrap().name != name {
        press(app, KeyCode::Down);
    }
}

fn editor_text(app: &App) -> String {
    app.state().editor().buffer().text()
}

fn tab_count(app: &App) -> usize {
    app.state().workspace().tab_names().0.len()
}

#[test]
fn ctrl_b_moves_the_keyboard_to_the_explorer_and_back() {
    let dir = project();
    let mut app = app_in(dir.path());
    assert_eq!(app.state().focus(), Focus::Editor);

    ctrl(&mut app, 'b');
    assert_eq!(app.state().focus(), Focus::Explorer);

    ctrl(&mut app, 'b');
    assert_eq!(app.state().focus(), Focus::Editor);
}

#[test]
fn escape_returns_from_the_explorer_to_the_editor() {
    let dir = project();
    let mut app = app_in(dir.path());
    ctrl(&mut app, 'b');

    press(&mut app, KeyCode::Esc);

    assert_eq!(app.state().focus(), Focus::Editor);
}

#[test]
fn typing_in_the_explorer_does_not_edit_the_document() {
    let dir = project();
    let mut app = app_in(dir.path());
    ctrl(&mut app, 'b');

    type_str(&mut app, "hello");

    assert_eq!(editor_text(&app), "");
    assert!(!app.state().editor().buffer().is_dirty());
}

#[test]
fn editor_bindings_do_not_apply_while_the_explorer_has_the_keyboard() {
    let dir = project();
    let mut app = app_in(dir.path());
    type_str(&mut app, "text");
    ctrl(&mut app, 'b');

    ctrl(&mut app, 'z');

    assert_eq!(editor_text(&app), "text", "ctrl+z is an editor binding");
}

#[test]
fn global_bindings_still_work_from_the_explorer() {
    let dir = project();
    let mut app = app_in(dir.path());
    ctrl(&mut app, 'b');

    ctrl(&mut app, 'q');

    assert!(app.should_quit());
}

#[test]
fn enter_opens_the_file_and_the_keyboard_stays_in_the_explorer() {
    let dir = project();
    let mut app = app_in(dir.path());
    ctrl(&mut app, 'b');
    select(&mut app, "a.txt");

    press(&mut app, KeyCode::Enter);

    assert_eq!(app.state().focus(), Focus::Explorer);
    assert_eq!(editor_text(&app), "alpha\n");
    assert_eq!(app.state().editor().display_name(), "a.txt");
}

#[test]
fn opening_several_files_in_a_row_opens_a_tab_each() {
    let dir = project();
    let mut app = app_in(dir.path());
    ctrl(&mut app, 'b');
    let before = tab_count(&app);

    select(&mut app, "a.txt");
    press(&mut app, KeyCode::Enter);
    select(&mut app, "b.txt");
    press(&mut app, KeyCode::Enter);

    assert_eq!(tab_count(&app), before + 2);
    assert_eq!(editor_text(&app), "beta\n");
}

#[test]
fn opening_a_file_that_is_already_open_switches_to_it() {
    let dir = project();
    let mut app = app_in(dir.path());
    ctrl(&mut app, 'b');
    select(&mut app, "a.txt");
    press(&mut app, KeyCode::Enter);
    select(&mut app, "b.txt");
    press(&mut app, KeyCode::Enter);
    let tabs = tab_count(&app);
    let documents = app.state().workspace().document_count();

    select(&mut app, "a.txt");
    press(&mut app, KeyCode::Enter);

    assert_eq!(tab_count(&app), tabs);
    assert_eq!(app.state().workspace().document_count(), documents);
    assert_eq!(editor_text(&app), "alpha\n");
}

#[test]
fn a_file_open_in_another_pane_is_focused_there_not_opened_twice() {
    let dir = project();
    let mut app = app_in(dir.path());
    ctrl(&mut app, 'b');
    select(&mut app, "a.txt");
    press(&mut app, KeyCode::Enter);
    ctrl(&mut app, 'k');
    press(&mut app, KeyCode::Right);
    select(&mut app, "b.txt");
    press(&mut app, KeyCode::Enter);
    let documents = app.state().workspace().document_count();

    select(&mut app, "a.txt");
    press(&mut app, KeyCode::Enter);

    assert_eq!(app.state().workspace().document_count(), documents);
    assert_eq!(editor_text(&app), "alpha\n");
}

#[test]
fn a_file_that_is_modified_is_not_read_again_when_chosen() {
    let dir = project();
    let mut app = app_in(dir.path());
    ctrl(&mut app, 'b');
    select(&mut app, "a.txt");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Esc);
    type_str(&mut app, "edited ");
    ctrl(&mut app, 'b');

    press(&mut app, KeyCode::Enter);

    assert_eq!(editor_text(&app), "edited alpha\n");
}

#[test]
fn opening_a_file_that_is_not_utf8_is_an_error_and_changes_nothing() {
    let dir = project();
    fs::write(dir.path().join("bin.dat"), [0xff, 0xfe, 0x00]).unwrap();
    let mut app = app_in(dir.path());
    ctrl(&mut app, 'b');
    select(&mut app, "bin.dat");
    let before = tab_count(&app);

    press(&mut app, KeyCode::Enter);

    assert!(app.state().notifications().has_errors());
    assert_eq!(tab_count(&app), before);
    assert_eq!(app.state().focus(), Focus::Explorer);
}

#[test]
fn right_and_left_open_and_close_directories() {
    let dir = project();
    let mut app = app_in(dir.path());
    ctrl(&mut app, 'b');
    select(&mut app, "src");

    press(&mut app, KeyCode::Right);
    assert!(
        app.state()
            .explorer()
            .rows()
            .iter()
            .any(|row| row.name == "main.rs")
    );

    press(&mut app, KeyCode::Left);
    assert!(
        !app.state()
            .explorer()
            .rows()
            .iter()
            .any(|row| row.name == "main.rs")
    );
}

#[test]
fn f5_reads_the_directories_again() {
    let dir = project();
    let mut app = app_in(dir.path());
    ctrl(&mut app, 'b');
    fs::write(dir.path().join("fresh.txt"), "").unwrap();

    press(&mut app, KeyCode::F(5));

    assert!(
        app.state()
            .explorer()
            .rows()
            .iter()
            .any(|row| row.name == "fresh.txt")
    );
}

#[test]
fn new_file_from_the_explorer_returns_the_keyboard_to_the_editor() {
    let dir = project();
    let mut app = app_in(dir.path());
    ctrl(&mut app, 'b');
    let before = tab_count(&app);

    ctrl(&mut app, 'n');

    assert_eq!(app.state().focus(), Focus::Editor);
    assert_eq!(tab_count(&app), before + 1);
}

#[test]
fn the_explorer_is_drawn_with_the_project_tree() {
    let dir = project();
    let mut app = app_in(dir.path());

    let screen = draw(&mut app);

    assert!(screen.contains("▸ src"), "{screen}");
    assert!(screen.contains("a.txt"), "{screen}");
    let root = dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(screen.contains(&root), "{screen}");
}

#[test]
fn clicking_a_file_in_the_explorer_opens_it_and_keeps_the_keyboard_there() {
    let dir = project();
    let mut app = app_in(dir.path());
    draw(&mut app);
    let row = app
        .state()
        .explorer()
        .rows()
        .iter()
        .position(|row| row.name == "a.txt")
        .unwrap() as u16;

    // The default layout hides the explorer title, so the tree starts on the first row.
    app.handle_input(click(3, row));

    assert_eq!(app.state().focus(), Focus::Explorer);
    assert_eq!(editor_text(&app), "alpha\n");
}

#[test]
fn clicking_the_editor_returns_the_keyboard_to_it() {
    let dir = project();
    let mut app = app_in(dir.path());
    draw(&mut app);
    ctrl(&mut app, 'b');

    app.handle_input(click(60, 10));

    assert_eq!(app.state().focus(), Focus::Editor);
}

#[test]
fn the_wheel_over_the_explorer_scrolls_it_not_the_editor() {
    let dir = tempfile::tempdir().unwrap();
    for number in 0..80 {
        fs::write(dir.path().join(format!("file{number:02}.txt")), "").unwrap();
    }
    let mut app = app_in(dir.path());
    draw(&mut app);

    for _ in 0..10 {
        app.handle_input(InputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 3,
            row: 5,
            modifiers: KeyModifiers::NONE,
        }));
    }
    let screen = draw(&mut app);

    assert!(
        !screen.contains("file00.txt"),
        "scrolled past the first files: {screen}"
    );
    assert!(screen.contains("file10.txt"), "{screen}");
}

#[test]
fn a_layout_without_an_explorer_cannot_give_it_the_keyboard() {
    let dir = project();
    let mut app = app_in(dir.path());
    let layout = LayoutTree::from_ron("Pane(view: Editor)").unwrap();
    assert!(!layout.contains(&ComponentKind::Explorer));
    app.state_mut().set_layout(layout);

    ctrl(&mut app, 'b');

    assert_eq!(app.state().focus(), Focus::Editor);
    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("no explorer")
    );
}

#[test]
fn the_palette_lists_the_explorer_actions_with_their_keys() {
    let dir = project();
    let mut app = app_in(dir.path());

    press(&mut app, KeyCode::F(1));
    type_str(&mut app, "explorer");

    let entries: Vec<_> = app
        .state()
        .palette()
        .unwrap()
        .visible()
        .map(|entry| (entry.title, entry.keys.clone()))
        .collect();
    assert!(
        entries.contains(&("Explorer: Focus", Some("ctrl+b".to_owned()))),
        "{entries:?}"
    );
    assert!(
        entries.contains(&("Explorer: Refresh", Some("f5".to_owned()))),
        "{entries:?}"
    );
}

#[test]
fn running_focus_from_the_palette_moves_the_keyboard() {
    let dir = project();
    let mut app = app_in(dir.path());
    press(&mut app, KeyCode::F(1));
    type_str(&mut app, "explorer focus");

    press(&mut app, KeyCode::Enter);

    assert_eq!(app.state().focus(), Focus::Explorer);
}
