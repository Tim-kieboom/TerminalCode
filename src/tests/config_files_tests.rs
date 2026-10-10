use std::fs;
use std::path::Path;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::config::ConfigFile;
use crate::event::action::Action;
use crate::keymap::DEFAULT_KEYMAP_TOML;
use crate::ui::layout::DEFAULT_LAYOUT_RON;
use crate::ui::theme::DEFAULT_THEME_TOML;

fn press(app: &mut App, code: KeyCode) {
    app.handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

/// Runs the palette entry named by `query`.
fn run_from_palette(app: &mut App, query: &str) {
    press(app, KeyCode::F(1));
    for c in query.chars() {
        press(app, KeyCode::Char(c));
    }
    press(app, KeyCode::Enter);
}

fn app_with_config_dir(dir: &Path) -> App {
    App::default().with_config_dir(Some(dir.to_path_buf()))
}

fn editor_text(app: &App) -> String {
    app.state().editor().buffer().text().to_string()
}

#[test]
fn the_palette_lists_the_three_config_files() {
    let actions = Action::palette_actions();

    for file in [ConfigFile::Theme, ConfigFile::Keymap, ConfigFile::Layout] {
        assert!(actions.contains(&Action::OpenConfig(file)), "{file:?}");
        assert!(Action::OpenConfig(file).title().is_some());
    }
}

#[test]
fn a_missing_file_is_made_from_the_default_and_opened() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Open Theme");

    let path = dir.path().join("theme.toml");
    assert_eq!(fs::read_to_string(&path).unwrap(), DEFAULT_THEME_TOML);
    assert_eq!(editor_text(&app), DEFAULT_THEME_TOML);
    assert_eq!(app.state().editor().buffer().path(), Some(path.as_path()));
}

#[test]
fn each_file_starts_from_its_own_default() {
    let cases = [
        ("Open Keymap", "keymap.toml", DEFAULT_KEYMAP_TOML),
        ("Open Layout", "layout.ron", DEFAULT_LAYOUT_RON),
    ];

    for (query, name, default) in cases {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_config_dir(dir.path());

        run_from_palette(&mut app, query);

        assert_eq!(fs::read_to_string(dir.path().join(name)).unwrap(), default);
    }
}

#[test]
fn an_existing_file_is_opened_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("theme.toml"),
        "[palette]\naccent = \"red\"\n",
    )
    .unwrap();
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Open Theme");

    assert_eq!(editor_text(&app), "[palette]\naccent = \"red\"\n");
    assert_eq!(
        fs::read_to_string(dir.path().join("theme.toml")).unwrap(),
        "[palette]\naccent = \"red\"\n"
    );
}

#[test]
fn the_config_directory_is_made_when_it_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("a").join("terminalcode");
    let mut app = app_with_config_dir(&config);

    run_from_palette(&mut app, "Open Layout");

    assert!(config.join("layout.ron").is_file());
}

#[test]
fn making_a_file_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Open Theme");

    let message = app.state().latest_notification().unwrap();
    assert!(message.contains("theme.toml"), "{message}");
    assert!(message.contains("defaults"), "{message}");
}

#[test]
fn opening_an_existing_file_twice_shows_the_same_tab() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());

    run_from_palette(&mut app, "Open Theme");
    let after_first = app.state().workspace().tab_names().0.len();

    run_from_palette(&mut app, "Open Theme");

    assert_eq!(app.state().workspace().tab_names().0.len(), after_first);
}

#[test]
fn without_a_config_directory_it_says_so_and_makes_nothing() {
    let mut app = App::default();

    run_from_palette(&mut app, "Open Theme");

    let message = app.state().latest_notification().unwrap();
    assert!(message.contains("no config directory"), "{message}");
}

#[test]
fn a_directory_that_cannot_be_made_is_reported_with_its_path() {
    let dir = tempfile::tempdir().unwrap();
    // A file where the config directory should be.
    let blocker = dir.path().join("terminalcode");
    fs::write(&blocker, "").unwrap();
    let mut app = app_with_config_dir(&blocker);

    run_from_palette(&mut app, "Open Theme");

    let message = app.state().latest_notification().unwrap();
    assert!(message.contains("terminalcode"), "{message}");
}

#[test]
fn an_action_can_be_bound_in_a_keymap_file() {
    let action: Action = toml::from_str::<Wrapper>("action = { open_config = \"theme\" }")
        .unwrap()
        .action;

    assert_eq!(action, Action::OpenConfig(ConfigFile::Theme));
}

#[derive(serde::Deserialize)]
struct Wrapper {
    action: Action,
}

// ---- reload

use crate::app::state::Focus;
use crate::components::ComponentKind;
use crate::ui::theme::{Rgb, Theme};
use ratatui::style::Color;

fn write(dir: &Path, name: &str, text: &str) {
    fs::write(dir.join(name), text).unwrap();
}

fn reload(app: &mut App) {
    run_from_palette(app, "Reload");
}

#[test]
fn the_palette_lists_reload() {
    assert!(Action::palette_actions().contains(&Action::Reload));
    assert!(Action::Reload.title().is_some());
}

#[test]
fn reload_applies_a_changed_theme() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    let before = app.state().theme().style("pane.border.focused");
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );

    reload(&mut app);

    let after = app.state().theme().style("pane.border.focused");
    assert_ne!(after, before);
    assert_eq!(after.fg, Some(Color::Rgb(0, 255, 0)));
}

#[test]
fn reload_applies_a_changed_layout() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    assert!(app.state().layout().contains(&ComponentKind::Explorer));
    write(dir.path(), "layout.ron", "Pane(view: Editor)");

    reload(&mut app);

    assert!(!app.state().layout().contains(&ComponentKind::Explorer));
}

#[test]
fn reload_applies_a_changed_keymap() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(
        dir.path(),
        "keymap.toml",
        "[[binding]]\nkeys = \"f2\"\naction = \"quit\"\n",
    );
    press(&mut app, KeyCode::F(2));
    assert!(!app.should_quit(), "not bound before the reload");

    reload(&mut app);
    press(&mut app, KeyCode::F(2));

    assert!(app.should_quit());
}

#[test]
fn reload_says_what_it_did() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());

    reload(&mut app);

    let message = app.state().latest_notification().unwrap();
    assert!(message.contains("reloaded"), "{message}");
}

#[test]
fn a_removed_file_goes_back_to_the_default_on_reload() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    let default = app.state().theme().clone();
    write(
        dir.path(),
        "theme.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );
    reload(&mut app);
    assert_ne!(*app.state().theme(), default);

    fs::remove_file(dir.path().join("theme.toml")).unwrap();
    reload(&mut app);

    assert_eq!(*app.state().theme(), default);
}

#[test]
fn an_invalid_file_is_reported_and_the_other_files_are_still_applied() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    write(dir.path(), "theme.toml", "[\"a\"]\ntext = \"@nope\"\n");
    write(dir.path(), "layout.ron", "Pane(view: Editor)");

    reload(&mut app);

    let message = app.state().latest_notification().unwrap();
    assert!(message.contains("theme.toml"), "{message}");
    assert_eq!(*app.state().theme(), Theme::default());
    assert!(!app.state().layout().contains(&ComponentKind::Explorer));
}

#[test]
fn reload_keeps_the_terminal_background_learned_at_startup() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    let mut theme = Theme::default();
    theme.set_terminal_background(Some(Rgb { r: 1, g: 2, b: 3 }));
    app.state_mut().set_theme(theme);

    reload(&mut app);

    assert_eq!(
        app.state().theme().terminal_background(),
        Some(Rgb { r: 1, g: 2, b: 3 })
    );
}

#[test]
fn the_keyboard_leaves_a_component_the_new_layout_lacks() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    app.state_mut().set_focus(Focus::Explorer);
    write(dir.path(), "layout.ron", "Pane(view: Editor)");

    reload(&mut app);

    assert_eq!(app.state().focus(), Focus::Editor);
}

#[test]
fn reload_leaves_open_files_alone() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with_config_dir(dir.path());
    press(&mut app, KeyCode::Char('x'));
    let text = editor_text(&app);

    reload(&mut app);

    assert_eq!(editor_text(&app), text);
}

#[test]
fn reload_with_a_syntax_worker_running_starts_a_new_one_without_trouble() {
    let dir = tempfile::tempdir().unwrap();
    let (events, _results) = tokio::sync::mpsc::channel(16);
    let mut app = app_with_config_dir(dir.path()).with_events(events);
    write(dir.path(), "theme.toml", "[palette]\nteal = \"#00ff00\"\n");

    reload(&mut app);

    assert!(app.state_mut().refresh_highlights().is_empty());
    let message = app.state().latest_notification().unwrap();
    assert!(message.contains("reloaded"), "{message}");
}
