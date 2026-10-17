//! Putting a config file back to the built-in default: asked first, the old
//! file kept as `NAME.bak`, the default applied.

use std::fs;
use std::path::Path;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::app::state::Popup;
use crate::config::ConfigFile;
use crate::event::action::Action;
use crate::persist::saved_state;

fn press(app: &mut App, code: KeyCode) {
    app.handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn run_from_palette(app: &mut App, query: &str) {
    press(app, KeyCode::F(1));
    for c in query.chars() {
        press(app, KeyCode::Char(c));
    }
    press(app, KeyCode::Enter);
}

fn app_in(dir: &Path) -> App {
    App::default().with_config_dir(Some(dir.to_path_buf()))
}

fn asking(app: &App) -> bool {
    matches!(app.state().popup(), Popup::Confirm(_))
}

/// Asks for the reset from the palette and says yes.
fn reset(app: &mut App, query: &str) {
    run_from_palette(app, query);
    assert!(asking(app), "the reset asks first");
    press(app, KeyCode::Char('y'));
    assert!(!asking(app));
}

const CASES: [(&str, ConfigFile, &str); 4] = [
    ("Reset Settings", ConfigFile::Settings, "settings.json"),
    ("Reset Keymap", ConfigFile::Keymap, "keymap.toml"),
    ("Reset Layout", ConfigFile::Layout, "layout.ron"),
    ("Reset Theme", ConfigFile::Theme, "theme.toml"),
];

#[test]
fn every_config_file_and_the_remembered_panes_have_a_reset_in_the_palette() {
    let actions = Action::palette_actions();

    for (_, file, _) in CASES {
        assert!(actions.contains(&Action::ResetConfig(file)), "{file:?}");
        assert!(Action::ResetConfig(file).title().unwrap().contains("Reset"));
    }
    assert!(actions.contains(&Action::ResetSavedState));
}

#[test]
fn a_reset_asks_first_and_only_y_says_yes() {
    for answer in [KeyCode::Enter, KeyCode::Esc, KeyCode::Char('n')] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("settings.json"), "mine").unwrap();
        let mut app = app_in(dir.path());

        run_from_palette(&mut app, "Reset Settings");
        assert!(asking(&app));
        press(&mut app, answer);

        assert!(!asking(&app));
        assert_eq!(
            fs::read_to_string(dir.path().join("settings.json")).unwrap(),
            "mine"
        );
        assert!(!dir.path().join("settings.json.bak").exists());
    }
}

#[test]
fn the_question_names_the_file_and_its_backup() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());

    run_from_palette(&mut app, "Reset Layout");

    let Popup::Confirm(confirm) = app.state().popup() else {
        panic!("a question");
    };
    let text = confirm.text();
    assert!(text.contains("layout.ron"), "{text}");
    assert!(text.contains("layout.ron.bak"), "{text}");
}

#[test]
fn saying_yes_keeps_the_old_file_as_bak_and_writes_the_default() {
    for (query, file, name) in CASES {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(name), "my own edits").unwrap();
        let mut app = app_in(dir.path());

        reset(&mut app, query);

        let backup = fs::read_to_string(dir.path().join(format!("{name}.bak"))).unwrap();
        assert_eq!(backup, "my own edits", "{name}");
        let now = fs::read_to_string(dir.path().join(name)).unwrap();
        assert_eq!(now, file.default_content(), "{name}");
        let told = app.state().latest_notification().unwrap_or_default();
        assert!(told.contains(&format!("{name}.bak")), "{told}");
    }
}

#[test]
fn a_second_reset_replaces_the_older_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, "first edits").unwrap();
    let mut app = app_in(dir.path());
    reset(&mut app, "Reset Settings");

    fs::write(&path, "second edits").unwrap();
    reset(&mut app, "Reset Settings");

    let backup = fs::read_to_string(dir.path().join("settings.json.bak")).unwrap();
    assert_eq!(backup, "second edits");
}

#[test]
fn a_missing_file_just_gets_the_default_and_no_backup() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());

    reset(&mut app, "Reset Keymap");

    assert!(dir.path().join("keymap.toml").exists());
    assert!(!dir.path().join("keymap.toml.bak").exists());
}

#[test]
fn a_reset_applies_the_default_at_once() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("settings.json"), r#"{ "mouse": false }"#).unwrap();
    let mut app = app_in(dir.path());
    run_from_palette(&mut app, "Reload");
    assert_eq!(app.take_mouse_change(), Some(false));

    reset(&mut app, "Reset Settings");

    assert_eq!(app.take_mouse_change(), Some(true));
}

#[test]
fn without_a_config_folder_it_says_so_and_asks_nothing() {
    let mut app = App::default();

    run_from_palette(&mut app, "Reset Settings");

    assert!(!asking(&app));
    let told = app.state().latest_notification().unwrap_or_default();
    assert!(told.contains("config directory"), "{told}");
}

#[test]
fn a_file_that_cannot_be_moved_is_left_alone_and_the_failure_is_shown() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("settings.json"), "mine").unwrap();
    // A folder with something in it is where the backup would go.
    let blocker = dir.path().join("settings.json.bak");
    fs::create_dir(&blocker).unwrap();
    fs::write(blocker.join("inside"), "").unwrap();
    let mut app = app_in(dir.path());

    run_from_palette(&mut app, "Reset Settings");
    press(&mut app, KeyCode::Char('y'));

    assert_eq!(
        fs::read_to_string(dir.path().join("settings.json")).unwrap(),
        "mine"
    );
    let told = app.state().latest_notification().unwrap_or_default();
    assert!(told.contains("not reset"), "{told}");
}

#[test]
fn forgetting_the_panes_keeps_state_json_as_bak_and_leaves_the_screen_alone() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    // Hide the explorer: ctrl+k b.
    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('k'),
        KeyModifiers::CONTROL,
    )));
    press(&mut app, KeyCode::Char('b'));
    let before = fs::read_to_string(saved_state::path_in(dir.path())).unwrap();

    reset(&mut app, "Forget Hidden");

    let backup = dir.path().join("state.json.bak");
    assert_eq!(fs::read_to_string(backup).unwrap(), before);
    let now = fs::read_to_string(saved_state::path_in(dir.path())).unwrap();
    assert_eq!(now, saved_state::empty_text());
    assert!(!app.state().is_visible(&crate::ui::ComponentKind::Explorer));
    let (state, warning) = saved_state::load(Some(dir.path()));
    assert!(warning.is_none());
    assert_eq!(state, saved_state::SavedState::default());
}

#[test]
fn what_is_hidden_after_a_forget_is_written_again() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    reset(&mut app, "Forget Hidden");

    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('k'),
        KeyModifiers::CONTROL,
    )));
    press(&mut app, KeyCode::Char('s'));

    let text = fs::read_to_string(saved_state::path_in(dir.path())).unwrap();
    assert!(text.contains("status_bar"), "{text}");
    assert!(!text.contains("explorer"), "{text}");
}

#[test]
fn a_reset_works_with_every_tab_closed() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("settings.json"), "mine").unwrap();
    let mut app = app_in(dir.path());
    run_from_palette(&mut app, "Close All");
    assert!(!app.state().workspace().has_tabs() || app.state().editor().buffer().path().is_none());

    reset(&mut app, "Reset Settings");

    assert!(dir.path().join("settings.json.bak").exists());
}
