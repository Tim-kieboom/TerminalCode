//! What is hidden is written to `state.json` as it changes, and read back at
//! the next start.

use std::fs;
use std::path::Path;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::{AppState, Focus};
use crate::persist::saved_state::{self, SavedState, load, path_in, save};
use crate::pty::Shell;
use crate::startup;
use crate::terminal::KeyboardSupport;
use crate::ui::ComponentKind;

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

fn dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn nothing_is_remembered_without_a_file_or_a_folder() {
    let empty = dir();

    for (state, warning) in [load(Some(empty.path())), load(None)] {
        assert_eq!(state, SavedState::default());
        assert!(warning.is_none());
    }
}

#[test]
fn what_was_saved_is_read_back() {
    let dir = dir();
    let mut state = SavedState::default();
    state.set_shown(&ComponentKind::Explorer, false);
    state.set_shown(&ComponentKind::Terminal, true);

    save(dir.path(), &state).unwrap();
    let (loaded, warning) = load(Some(dir.path()));

    assert!(warning.is_none());
    assert_eq!(loaded.shown(&ComponentKind::Explorer), Some(false));
    assert_eq!(loaded.shown(&ComponentKind::Terminal), Some(true));
    assert_eq!(loaded.shown(&ComponentKind::StatusBar), None);
}

#[test]
fn only_the_panes_that_were_changed_are_in_the_file() {
    let dir = dir();
    let mut state = SavedState::default();
    state.set_shown(&ComponentKind::Terminal, true);

    save(dir.path(), &state).unwrap();

    let text = fs::read_to_string(path_in(dir.path())).unwrap();
    assert!(text.contains("terminal"), "{text}");
    assert!(!text.contains("explorer"), "{text}");
    assert!(!text.contains("status_bar"), "{text}");
}

#[test]
fn setting_a_pane_says_whether_it_is_news() {
    let mut state = SavedState::default();

    assert!(state.set_shown(&ComponentKind::Explorer, false));
    assert!(!state.set_shown(&ComponentKind::Explorer, false));
    assert!(state.set_shown(&ComponentKind::Explorer, true));
}

#[test]
fn the_editor_and_plugin_views_are_not_remembered() {
    let mut state = SavedState::default();

    assert!(!state.set_shown(&ComponentKind::Editor, false));
    assert_eq!(state.shown(&ComponentKind::Editor), None);
}

#[test]
fn a_file_that_is_not_valid_gives_the_empty_state_and_a_warning_naming_it() {
    let dir = dir();
    fs::write(path_in(dir.path()), "{ not json").unwrap();

    let (state, warning) = load(Some(dir.path()));

    assert_eq!(state, SavedState::default());
    let warning = warning.expect("a warning").to_string();
    assert!(warning.contains("state.json"), "{warning}");
}

#[test]
fn a_file_with_the_wrong_types_gives_a_warning_too() {
    let dir = dir();
    fs::write(path_in(dir.path()), r#"{ "shown": { "explorer": "no" } }"#).unwrap();

    let (state, warning) = load(Some(dir.path()));

    assert_eq!(state, SavedState::default());
    assert!(warning.is_some());
}

#[test]
fn names_this_version_does_not_know_are_ignored() {
    let dir = dir();
    fs::write(
        path_in(dir.path()),
        r#"{ "shown": { "explorer": false, "sidebar": true }, "later": 1 }"#,
    )
    .unwrap();

    let (state, warning) = load(Some(dir.path()));

    assert!(warning.is_none());
    assert_eq!(state.shown(&ComponentKind::Explorer), Some(false));
}

#[test]
fn saving_leaves_no_temporary_file_and_replaces_the_old_one() {
    let dir = dir();
    let mut state = SavedState::default();
    state.set_shown(&ComponentKind::Explorer, false);
    save(dir.path(), &state).unwrap();
    state.set_shown(&ComponentKind::Explorer, true);

    save(dir.path(), &state).unwrap();

    let files: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(files, ["state.json"]);
    assert_eq!(
        load(Some(dir.path())).0.shown(&ComponentKind::Explorer),
        Some(true)
    );
}

#[test]
fn a_failed_save_leaves_the_old_file_as_it_was() {
    let dir = dir();
    let mut state = SavedState::default();
    state.set_shown(&ComponentKind::Explorer, false);
    save(dir.path(), &state).unwrap();
    let before = fs::read_to_string(path_in(dir.path())).unwrap();
    // A folder where the temporary file goes makes the write fail.
    fs::create_dir(dir.path().join("state.json.tmp")).unwrap();
    state.set_shown(&ComponentKind::Explorer, true);

    assert!(save(dir.path(), &state).is_err());

    assert_eq!(fs::read_to_string(path_in(dir.path())).unwrap(), before);
}

#[test]
fn saving_makes_the_config_folder_if_it_is_missing() {
    let dir = dir();
    let nested = dir.path().join("a/b");

    save(&nested, &SavedState::default()).unwrap();

    assert!(path_in(&nested).exists());
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

#[cfg(unix)]
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
