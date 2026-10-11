//! Closing several tabs, reopening a closed one and saving every file, from
//! the keys and the palette.

use std::fs;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::app::state::Focus;
use crate::event::action::Action;

fn key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
}

/// `ctrl+k` and then a letter.
fn chord(app: &mut App, letter: char) {
    key(app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    key(app, KeyCode::Char(letter), KeyModifiers::NONE);
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        key(app, KeyCode::Char(c), KeyModifiers::NONE);
    }
}

fn project() -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(dir.path().join(name), format!("{name}\n")).unwrap();
    }
    let mut app = App::default();
    for name in ["a.txt", "b.txt", "c.txt"] {
        app.state_mut()
            .workspace_mut()
            .open_path(&dir.path().join(name))
            .unwrap();
    }
    (dir, app)
}

fn tabs(app: &App) -> Vec<String> {
    app.state().workspace().tab_names().0
}

fn said(app: &App, text: &str) -> bool {
    app.state()
        .notifications()
        .texts()
        .iter()
        .any(|shown| shown.contains(text))
}

#[test]
fn the_palette_lists_the_new_tab_actions() {
    let actions = Action::palette_actions();
    for action in [
        Action::CloseOtherTabs,
        Action::CloseAllTabs,
        Action::ReopenClosedTab,
        Action::SaveAll,
    ] {
        assert!(actions.contains(&action), "{action:?}");
        assert!(action.title().is_some());
    }
}

#[test]
fn ctrl_k_e_closes_the_other_tabs() {
    let (_dir, mut app) = project();

    chord(&mut app, 'e');

    assert_eq!(tabs(&app), ["c.txt"]);
}

#[test]
fn ctrl_k_w_closes_every_tab() {
    let (_dir, mut app) = project();

    chord(&mut app, 'w');

    assert!(tabs(&app).is_empty());
}

#[test]
fn closing_with_unsaved_changes_asks_first_and_the_same_chord_confirms() {
    let (_dir, mut app) = project();
    type_str(&mut app, "x");

    chord(&mut app, 'w');
    assert_eq!(tabs(&app).len(), 4, "the empty first tab and three files");
    assert!(said(&app, "c.txt [+] would lose unsaved changes"));

    chord(&mut app, 'w');
    assert!(tabs(&app).is_empty());
}

#[test]
fn ctrl_k_z_reopens_what_was_closed() {
    let (_dir, mut app) = project();
    chord(&mut app, 'w');

    chord(&mut app, 'z');

    assert_eq!(tabs(&app), ["c.txt"]);
}

#[test]
fn reopening_with_nothing_closed_says_so() {
    let mut app = App::default();

    chord(&mut app, 'z');

    assert!(said(&app, "no closed tab to reopen"));
}

#[test]
fn reopening_a_file_that_is_gone_is_an_error() {
    let (dir, mut app) = project();
    chord(&mut app, 'w');
    fs::remove_file(dir.path().join("c.txt")).unwrap();

    chord(&mut app, 'z');

    assert!(app.state().notifications().has_errors());
}

#[test]
fn reopening_gives_the_keyboard_to_the_editor() {
    let (_dir, mut app) = project();
    chord(&mut app, 'w');
    app.state_mut().set_focus(Focus::Explorer);

    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('k'),
        KeyModifiers::CONTROL,
    )));
    key(&mut app, KeyCode::Char('z'), KeyModifiers::NONE);

    assert_eq!(app.state().focus(), Focus::Editor);
}

#[test]
fn ctrl_k_a_saves_every_modified_file_and_says_how_many() {
    let (dir, mut app) = project();
    app.state_mut().workspace_mut().activate_tab(1);
    type_str(&mut app, "1");
    app.state_mut().workspace_mut().activate_tab(2);
    type_str(&mut app, "2");

    chord(&mut app, 'a');

    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "1a.txt\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("b.txt")).unwrap(),
        "2b.txt\n"
    );
    assert!(said(&app, "saved 2 files"));
}

#[test]
fn save_all_with_nothing_to_save_says_so() {
    let (_dir, mut app) = project();

    chord(&mut app, 'a');

    assert!(said(&app, "nothing to save"));
}

#[test]
fn save_all_names_the_untitled_document_it_left_alone() {
    let (dir, mut app) = project();
    app.state_mut().workspace_mut().activate_tab(0);
    type_str(&mut app, "scratch");
    app.state_mut().workspace_mut().activate_tab(1);
    type_str(&mut app, "1");

    chord(&mut app, 'a');

    assert!(said(&app, "[no name] [+] not saved: it has no file yet"));
    assert!(said(&app, "saved 1 of 2 modified files"));
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "1a.txt\n"
    );
}

#[test]
fn save_all_works_with_no_tabs_open() {
    let mut app = App::default();
    chord(&mut app, 'w');
    assert!(tabs(&app).is_empty());

    chord(&mut app, 'a');

    assert!(said(&app, "nothing to save"));
}
