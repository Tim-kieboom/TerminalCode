//! Open File and Open Folder: a prompt for a path. A file opens as a tab; a folder
//! ends the editor with the folder to start again in.

use std::fs;
use std::path::Path;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::app::expand_path;
use crate::app::state::AppState;
use crate::app::state::Popup;
use crate::buffer::Buffer;
use crate::components::editor::Editor;
use crate::event::action::Action;

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c), KeyModifiers::NONE);
    }
}

struct Rig {
    app: App,
    dir: tempfile::TempDir,
}

fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "alpha\n").unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub/b.txt"), "beta\n").unwrap();
    let mut state = AppState::new(Editor::new(Buffer::default()));
    state.open_project(dir.path()).unwrap();
    Rig {
        app: App::new(state),
        dir,
    }
}

fn popup_is_name_prompt(state: &AppState) -> bool {
    matches!(state.popup(), Popup::NewEntry(_))
}

fn open_text(app: &App) -> String {
    app.state().editor().buffer().text()
}

// ---- the path

#[test]
fn a_path_is_expanded_from_home_or_from_the_base() {
    let base = Path::new("/project");
    let home = dirs::home_dir().unwrap_or_default();

    assert_eq!(expand_path("~", base), home);
    assert_eq!(expand_path("~/notes.txt", base), home.join("notes.txt"));
    assert_eq!(
        expand_path("  src/a.rs ", base),
        Path::new("/project/src/a.rs")
    );
    assert_eq!(expand_path("/etc/hosts", base), Path::new("/etc/hosts"));
}

// ---- Open File

#[test]
fn open_file_asks_for_a_path_and_opens_it_as_a_tab_relative_to_the_project() {
    let mut rig = rig();

    press(&mut rig.app, KeyCode::Char('o'), KeyModifiers::CONTROL);
    assert!(popup_is_name_prompt(rig.app.state()));
    type_str(&mut rig.app, "sub/b.txt");
    press(&mut rig.app, KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(open_text(&rig.app), "beta\n");
    assert!(!popup_is_name_prompt(rig.app.state()));
}

#[test]
fn an_absolute_path_opens_too() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFile);

    type_str(
        &mut rig.app,
        &rig.dir.path().join("a.txt").display().to_string(),
    );
    press(&mut rig.app, KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(open_text(&rig.app), "alpha\n");
}

#[test]
fn a_path_that_does_not_exist_is_said_and_the_prompt_stays_for_another_try() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFile);

    type_str(&mut rig.app, "nope.txt");
    press(&mut rig.app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(popup_is_name_prompt(rig.app.state()));
    let told = rig.app.state().notifications().texts().join("|");
    assert!(told.contains("nope.txt"), "{told}");
}

#[test]
fn a_folder_is_not_a_file_and_an_empty_path_is_refused() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFile);

    press(&mut rig.app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(popup_is_name_prompt(rig.app.state()));

    type_str(&mut rig.app, "sub");
    press(&mut rig.app, KeyCode::Enter, KeyModifiers::NONE);
    let told = rig.app.state().notifications().texts().join("|");
    assert!(told.contains("is not a file"), "{told}");
}

// ---- Open Folder

#[test]
fn open_folder_ends_the_editor_with_the_folder_to_start_again_in() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFolder);

    type_str(&mut rig.app, "sub");
    press(&mut rig.app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(rig.app.should_quit());
    let target = rig.app.restart_target().unwrap();
    assert_eq!(
        target,
        crate::paths::canonical(&rig.dir.path().join("sub")).as_path()
    );
}

#[test]
fn a_file_is_not_a_folder_and_nothing_ends() {
    let mut rig = rig();
    rig.app.do_action(Action::OpenFolder);

    type_str(&mut rig.app, "a.txt");
    press(&mut rig.app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(!rig.app.should_quit());
    assert!(rig.app.restart_target().is_none());
    let told = rig.app.state().notifications().texts().join("|");
    assert!(told.contains("is not a folder"), "{told}");
}

#[test]
fn unsaved_files_are_asked_about_first_and_cancelling_stays_in_the_editor() {
    let mut rig = rig();
    type_str(&mut rig.app, "unsaved");
    rig.app.do_action(Action::OpenFolder);
    type_str(&mut rig.app, "sub");
    press(&mut rig.app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(!rig.app.should_quit(), "the quit prompt is up");
    assert!(rig.app.restart_target().is_some());

    press(&mut rig.app, KeyCode::Esc, KeyModifiers::NONE);

    assert!(!rig.app.should_quit());
    assert!(
        rig.app.restart_target().is_none(),
        "a cancelled restart is dropped"
    );
}

#[test]
fn discarding_the_unsaved_files_goes_on_to_the_new_folder() {
    let mut rig = rig();
    type_str(&mut rig.app, "unsaved");
    rig.app.do_action(Action::OpenFolder);
    type_str(&mut rig.app, "sub");
    press(&mut rig.app, KeyCode::Enter, KeyModifiers::NONE);

    // The prompt's "discard all" key.
    press(&mut rig.app, KeyCode::Char('d'), KeyModifiers::NONE);

    assert!(rig.app.should_quit());
    assert!(rig.app.restart_target().is_some());
}

#[test]
fn a_plain_quit_has_nothing_to_start_again() {
    let mut rig = rig();

    rig.app.do_action(Action::Quit);

    assert!(rig.app.should_quit());
    assert!(rig.app.restart_target().is_none());
}
