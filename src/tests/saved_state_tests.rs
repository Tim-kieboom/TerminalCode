//! What is hidden is written to `state.json` as it changes, and read back at
//! the next start.

use std::fs;
use std::path::Path;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::{AppState, Focus};
use crate::components::ComponentKind;
use crate::pty::Shell;
use crate::saved_state;
use crate::startup;
use crate::terminal::KeyboardSupport;

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
}

/// `ctrl+k` then `key`: the chords that hide and show the explorer and the status bar.
fn chord(app: &mut App, key: char) {
    press(app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    press(app, KeyCode::Char(key), KeyModifiers::NONE);
}

/// An app started the way `run` starts one, with the files of `dir`.
fn started_in(dir: &Path) -> App {
    let mut state = AppState::default();
    let user = startup::load_user_config(&mut state, Some(dir), KeyboardSupport::Enhanced);
    App::with_keymap(state, user.keymap)
        .with_config_dir(Some(dir.to_path_buf()))
        .with_settings(user.settings, user.theme)
        .with_saved_state(user.saved)
}

fn written(dir: &Path) -> Option<String> {
    fs::read_to_string(saved_state::path_in(dir)).ok()
}

#[test]
fn nothing_is_written_until_something_is_hidden_or_shown() {
    let dir = tempfile::tempdir().unwrap();

    let mut app = started_in(dir.path());
    press(&mut app, KeyCode::Char('a'), KeyModifiers::NONE);

    assert_eq!(written(dir.path()), None);
}

#[test]
fn hiding_the_explorer_writes_it_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = started_in(dir.path());

    chord(&mut app, 'b');

    assert!(!app.state().is_visible(&ComponentKind::Explorer));
    let text = written(dir.path()).expect("state.json");
    assert!(text.contains("\"explorer\": false"), "{text}");
}

#[test]
fn only_the_panes_that_were_changed_are_written() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = started_in(dir.path());

    chord(&mut app, 's');

    let text = written(dir.path()).expect("state.json");
    assert!(text.contains("status_bar"), "{text}");
    assert!(!text.contains("explorer"), "{text}");
}

#[test]
fn showing_it_again_is_remembered_too() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = started_in(dir.path());
    chord(&mut app, 'b');

    chord(&mut app, 'b');

    let text = written(dir.path()).expect("state.json");
    assert!(text.contains("\"explorer\": true"), "{text}");
}

#[test]
fn the_next_start_has_the_panes_as_they_were_left() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = started_in(dir.path());
    chord(&mut app, 'b');
    chord(&mut app, 's');

    let next = started_in(dir.path());

    assert!(!next.state().is_visible(&ComponentKind::Explorer));
    assert!(!next.state().is_visible(&ComponentKind::StatusBar));
}

#[test]
fn a_reload_of_the_config_files_changes_nothing_that_is_hidden() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("settings.json"), "{}").unwrap();
    let mut app = started_in(dir.path());
    chord(&mut app, 'b');
    fs::write(dir.path().join("settings.json"), r#"{ "mouse": false }"#).unwrap();

    press(&mut app, KeyCode::F(1), KeyModifiers::NONE);
    for c in "Reload".chars() {
        press(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
    }
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(!app.state().is_visible(&ComponentKind::Explorer));
}

#[test]
fn without_a_config_folder_nothing_is_written_and_nothing_goes_wrong() {
    let mut app = App::default();

    chord(&mut app, 'b');

    assert!(!app.state().is_visible(&ComponentKind::Explorer));
    assert!(app.state().latest_notification().is_none());
}

#[test]
fn a_failed_write_is_told_once_per_run_not_at_every_toggle() {
    let dir = tempfile::tempdir().unwrap();
    // The config folder is a file, so nothing can be written in it.
    let blocked = dir.path().join("config");
    fs::write(&blocked, "").unwrap();
    let mut app = App::default().with_config_dir(Some(blocked));
    let told = |app: &App| {
        app.state()
            .notifications()
            .texts()
            .into_iter()
            .filter(|text| text.contains("will not be remembered"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };

    chord(&mut app, 'b');
    let first = told(&app);
    assert_eq!(first.len(), 1, "{first:?}");
    assert!(first[0].contains("state.json"), "{first:?}");

    // The next keys dismiss it; the toggles after it say nothing more.
    chord(&mut app, 'b');
    chord(&mut app, 's');
    assert_eq!(told(&app), Vec::<String>::new());
}

#[test]
fn a_terminal_that_starts_shown_gets_its_shell_and_the_keyboard_stays_put() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("layout.ron"),
        "Col([Pane(view: Editor), Fixed(10, Pane(view: Terminal, hidden: false))])",
    )
    .unwrap();
    let mut app = started_in(dir.path()).with_terminal_shell(Shell::script("sleep 30"));
    let (events, _receiver) = mpsc::channel(64);
    app = app.with_events(events);
    assert!(!app.state().terminal().is_running());

    app.start_terminal_if_shown();

    assert!(app.state().terminal().is_running());
    assert_eq!(app.state().focus(), Focus::Editor);
}

#[test]
fn a_terminal_that_starts_hidden_gets_no_shell() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = started_in(dir.path()).with_terminal_shell(Shell::script("sleep 30"));
    let (events, _receiver) = mpsc::channel(64);
    app = app.with_events(events);

    app.start_terminal_if_shown();

    assert!(!app.state().terminal().is_running());
}
