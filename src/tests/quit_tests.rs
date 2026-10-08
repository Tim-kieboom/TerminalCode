use std::fs;
use std::path::PathBuf;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::components::prepare_and_render;
use crate::initial_state;

fn press(code: KeyCode) -> InputEvent {
    InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn typed(c: char) -> InputEvent {
    let modifiers = if c.is_uppercase() {
        KeyModifiers::SHIFT
    } else {
        KeyModifiers::NONE
    };
    InputEvent::Key(KeyEvent::new(KeyCode::Char(c), modifiers))
}

fn ctrl(c: char) -> InputEvent {
    InputEvent::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
}

fn ctrl_q() -> InputEvent {
    InputEvent::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL))
}

/// An app with one tab per file, each containing `original\n`; the tabs in
/// `edited` get an extra `x` typed into them.
fn app_with_files(names: &[&str], edited: &[usize]) -> (App, tempfile::TempDir, Vec<PathBuf>) {
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<_> = names.iter().map(|name| dir.path().join(name)).collect();
    for path in &paths {
        fs::write(path, "original\n").unwrap();
    }
    let mut app = App::new(initial_state(&paths).unwrap());
    for &index in edited {
        app.state_mut().workspace_mut().activate_tab(index);
        app.handle_input(typed('x'));
    }
    (app, dir, paths)
}

fn on_disk(path: &PathBuf) -> String {
    fs::read_to_string(path).unwrap()
}

#[test]
fn quitting_with_nothing_modified_quits_at_once() {
    let (mut app, _dir, _) = app_with_files(&["a.txt", "b.txt"], &[]);

    app.handle_input(ctrl_q());

    assert!(app.should_quit());
    assert!(app.state().quit_prompt().is_none());
}

#[test]
fn quitting_with_unsaved_changes_asks_and_lists_only_modified_files() {
    let (mut app, _dir, _) = app_with_files(&["a.txt", "b.txt", "c.txt"], &[0, 2]);

    app.handle_input(ctrl_q());

    assert!(!app.should_quit());
    let names: Vec<_> = app
        .state()
        .quit_prompt()
        .unwrap()
        .items()
        .iter()
        .map(|item| item.name.as_str())
        .collect();
    assert_eq!(names, ["a.txt [+]", "c.txt [+]"]);
}

#[test]
fn escape_cancels_and_keeps_every_change() {
    let (mut app, _dir, paths) = app_with_files(&["a.txt"], &[0]);
    app.handle_input(ctrl_q());

    app.handle_input(press(KeyCode::Esc));

    assert!(!app.should_quit());
    assert!(app.state().quit_prompt().is_none());
    assert_eq!(on_disk(&paths[0]), "original\n");
    assert!(app.state().editor().buffer().is_dirty());
}

#[test]
fn the_prompt_takes_the_keys_instead_of_the_editor() {
    let (mut app, _dir, _) = app_with_files(&["a.txt"], &[0]);
    let before = app.state().editor().buffer().text();
    app.handle_input(ctrl_q());

    app.handle_input(typed('z'));
    app.handle_input(typed('q'));
    app.handle_input(ctrl_q());

    assert_eq!(app.state().editor().buffer().text(), before);
    assert!(!app.should_quit());
    assert!(app.state().quit_prompt().is_some());
}

#[test]
fn save_all_writes_every_file_and_quits() {
    let (mut app, _dir, paths) = app_with_files(&["a.txt", "b.txt", "c.txt"], &[0, 2]);
    app.handle_input(ctrl_q());

    app.handle_input(typed('s'));

    assert!(app.should_quit());
    assert_eq!(on_disk(&paths[0]), "xoriginal\n");
    assert_eq!(on_disk(&paths[1]), "original\n");
    assert_eq!(on_disk(&paths[2]), "xoriginal\n");
}

#[test]
fn discard_all_quits_without_writing() {
    let (mut app, _dir, paths) = app_with_files(&["a.txt", "b.txt"], &[0, 1]);
    app.handle_input(ctrl_q());

    app.handle_input(typed('d'));

    assert!(app.should_quit());
    assert_eq!(on_disk(&paths[0]), "original\n");
    assert_eq!(on_disk(&paths[1]), "original\n");
}

#[test]
fn saving_one_file_leaves_the_others_to_decide() {
    let (mut app, _dir, paths) = app_with_files(&["a.txt", "b.txt"], &[0, 1]);
    app.handle_input(ctrl_q());

    app.handle_input(ctrl('s'));

    assert!(!app.should_quit());
    assert_eq!(on_disk(&paths[0]), "xoriginal\n");
    assert_eq!(on_disk(&paths[1]), "original\n");
    let prompt = app.state().quit_prompt().unwrap();
    assert_eq!(prompt.items().len(), 1);
    assert_eq!(prompt.items()[0].name, "b.txt [+]");
}

#[test]
fn deciding_on_the_last_file_quits() {
    let (mut app, _dir, paths) = app_with_files(&["a.txt", "b.txt"], &[0, 1]);
    app.handle_input(ctrl_q());

    app.handle_input(ctrl('s'));
    app.handle_input(ctrl('d'));

    assert!(app.should_quit());
    assert_eq!(on_disk(&paths[0]), "xoriginal\n");
    assert_eq!(on_disk(&paths[1]), "original\n");
}

#[test]
fn arrows_choose_which_file_a_decision_applies_to() {
    let (mut app, _dir, paths) = app_with_files(&["a.txt", "b.txt", "c.txt"], &[0, 1, 2]);
    app.handle_input(ctrl_q());

    app.handle_input(press(KeyCode::Down));
    app.handle_input(ctrl('s'));

    assert_eq!(on_disk(&paths[0]), "original\n");
    assert_eq!(on_disk(&paths[1]), "xoriginal\n");
    assert_eq!(on_disk(&paths[2]), "original\n");
    let prompt = app.state().quit_prompt().unwrap();
    let names: Vec<_> = prompt.items().iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["a.txt [+]", "c.txt [+]"]);
    assert_eq!(
        prompt.selected(),
        1,
        "selection moves to what took its place"
    );
}

#[test]
fn the_selection_stops_at_both_ends() {
    let (mut app, _dir, _) = app_with_files(&["a.txt", "b.txt"], &[0, 1]);
    app.handle_input(ctrl_q());

    app.handle_input(press(KeyCode::Up));
    assert_eq!(app.state().quit_prompt().unwrap().selected(), 0);
    app.handle_input(press(KeyCode::Down));
    app.handle_input(press(KeyCode::Down));
    assert_eq!(app.state().quit_prompt().unwrap().selected(), 1);
}

#[test]
fn a_file_that_cannot_be_saved_stays_in_the_prompt_with_the_error() {
    let (mut app, _dir, _) = app_with_files(&["a.txt"], &[0]);
    app.state_mut().workspace_mut().new_file();
    app.handle_input(typed('y'));
    app.handle_input(ctrl_q());

    // The untitled file has no path, so saving everything stops there.
    app.handle_input(typed('s'));

    assert!(!app.should_quit());
    let prompt = app.state().quit_prompt().unwrap();
    assert_eq!(prompt.items().len(), 1);
    assert!(app.state().status().is_some());

    // It can still be discarded.
    app.handle_input(ctrl('d'));
    assert!(app.should_quit());
}

#[test]
fn the_prompt_is_drawn_over_the_editor() {
    let (mut app, _dir, _) = app_with_files(&["a.txt", "b.txt"], &[0, 1]);
    app.handle_input(ctrl_q());
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();

    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();

    let screen = terminal.backend().to_string();
    assert!(screen.contains("Unsaved changes"), "{screen}");
    assert!(screen.contains("save all"), "{screen}");
    assert!(screen.contains("b.txt"), "{screen}");
}
