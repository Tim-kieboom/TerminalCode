use std::fs;
use std::path::Path;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::config::{self, ConfigFile, ThemeChoice};
use crate::event::action::Action;
use crate::settings::{DEFAULT_SETTINGS_JSONC, Settings};
use crate::ui::theme::{Theme, ThemeVariant};

/// A slot the built-in themes color differently.
const SLOT: &str = "pane.border.focused";

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

fn write(dir: &Path, name: &str, text: &str) {
    fs::write(dir.join(name), text).unwrap();
}

fn reload(app: &mut App) {
    run_from_palette(app, "Reload");
}

fn app_in(dir: &Path) -> App {
    App::default().with_config_dir(Some(dir.to_path_buf()))
}

/// An app started the way `run` starts one with the settings file in `dir`.
fn started_with(dir: &Path, settings_text: &str) -> App {
    write(dir, "settings.json", settings_text);
    let loaded = config::load_settings(Some(&dir.join("settings.json")));
    let settings = loaded.settings.unwrap();
    let choice = config::resolve_theme(&settings.theme, Some(dir)).unwrap();
    app_in(dir).with_settings(settings, choice)
}

fn notified(app: &App, needle: &str) -> bool {
    app.state()
        .notifications()
        .texts()
        .iter()
        .any(|text| text.contains(needle))
}

// ---- loading

#[test]
fn no_settings_file_is_the_defaults_without_a_warning() {
    let dir = tempfile::tempdir().unwrap();

    for path in [None, Some(dir.path().join("settings.json"))] {
        let loaded = config::load_settings(path.as_deref());

        assert_eq!(loaded.settings, Some(Settings::default()));
        assert!(loaded.warning.is_none());
    }
}

#[test]
fn an_invalid_file_is_reported_with_its_path_and_gives_no_settings() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "settings.json", "{ \"theme\": 3 }");

    let loaded = config::load_settings(Some(&dir.path().join("settings.json")));

    assert_eq!(loaded.settings, None);
    let warning = loaded.warning.unwrap();
    assert!(warning.contains("settings.json"), "{warning}");
    assert!(warning.contains("\"theme\""), "{warning}");
}

#[test]
fn an_unknown_setting_is_reported_and_the_rest_applies() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "settings.json",
        "{ \"theme\": \"light\", \"colour\": 1 }",
    );

    let loaded = config::load_settings(Some(&dir.path().join("settings.json")));

    assert_eq!(loaded.settings.unwrap().theme, "light");
    assert!(loaded.warning.unwrap().contains("colour"));
}

// ---- the theme setting

#[test]
fn a_built_in_theme_is_found_by_name_or_title_in_any_case() {
    for name in ["soldark", "SolDark", "SOLDARK"] {
        let choice = config::resolve_theme(name, None).unwrap();
        assert_eq!(choice.variant, ThemeVariant::SolDark, "{name}");
        assert_eq!(choice.installed, None);
    }
}

#[test]
fn an_installed_theme_is_found_by_name() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("themes")).unwrap();
    write(&dir.path().join("themes"), "mine.toml", "");

    let choice = config::resolve_theme("mine", Some(dir.path())).unwrap();

    assert_eq!(choice.installed.as_deref(), Some("mine"));
}

#[test]
fn an_installed_theme_wins_over_a_built_in_one_of_the_same_name() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("themes")).unwrap();
    write(&dir.path().join("themes"), "dark.toml", "");

    let choice = config::resolve_theme("dark", Some(dir.path())).unwrap();

    assert_eq!(choice.installed.as_deref(), Some("dark"));
}

#[test]
fn a_name_that_is_no_theme_finds_nothing() {
    let dir = tempfile::tempdir().unwrap();

    assert_eq!(config::resolve_theme("nope", Some(dir.path())), None);
    assert_eq!(config::resolve_theme("../x", Some(dir.path())), None);
}

#[test]
fn the_app_starts_with_the_theme_the_settings_name() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = started_with(dir.path(), "{ \"theme\": \"light\" }");

    reload(&mut app);
    // Nothing changed in the file, so a reload leaves the started theme alone.

    assert_eq!(*app.state().theme(), Theme::built_in(ThemeVariant::Light),);
}

// ---- reload

#[test]
fn saving_the_settings_file_applies_its_theme() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());
    write(dir.path(), "settings.json", "{ \"theme\": \"light\" }");

    reload(&mut app);

    assert_eq!(*app.state().theme(), Theme::built_in(ThemeVariant::Light));
}

#[test]
fn a_theme_chosen_from_the_palette_survives_a_reload_that_leaves_the_setting_alone() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = started_with(dir.path(), "{ \"theme\": \"light\" }");
    run_from_palette(&mut app, "Theme: SolDark");

    write(dir.path(), "theme.toml", "# touched, but nothing in it\n");
    reload(&mut app);

    assert_ne!(*app.state().theme(), Theme::built_in(ThemeVariant::Light));
    let soldark = Theme::built_in(ThemeVariant::SolDark);
    assert_ne!(
        app.state().theme().style(SLOT),
        Theme::built_in(ThemeVariant::Light).style(SLOT)
    );
    assert_eq!(app.state().theme().style(SLOT), soldark.style(SLOT));
}

#[test]
fn changing_the_setting_again_applies_it_over_a_palette_choice() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = started_with(dir.path(), "{ \"theme\": \"light\" }");
    run_from_palette(&mut app, "Theme: SolDark");

    write(dir.path(), "settings.json", "{ \"theme\": \"sollight\" }");
    reload(&mut app);

    assert_eq!(
        app.state().theme().style(SLOT),
        Theme::built_in(ThemeVariant::SolLight).style(SLOT)
    );
}

#[test]
fn an_invalid_settings_file_leaves_the_theme_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = started_with(dir.path(), "{ \"theme\": \"light\" }");
    write(dir.path(), "settings.json", "{ \"theme\": ");

    reload(&mut app);

    assert_eq!(*app.state().theme(), Theme::built_in(ThemeVariant::Light));
    assert!(notified(&app, "using default settings"));
}

#[test]
fn a_theme_name_that_does_not_exist_keeps_the_theme_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = started_with(dir.path(), "{ \"theme\": \"light\" }");
    write(dir.path(), "settings.json", "{ \"theme\": \"nope\" }");

    reload(&mut app);

    assert_eq!(*app.state().theme(), Theme::built_in(ThemeVariant::Light));
    assert!(notified(&app, "no theme called \"nope\""));
}

#[test]
fn a_removed_settings_file_goes_back_to_the_default_theme() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = started_with(dir.path(), "{ \"theme\": \"light\" }");
    fs::remove_file(dir.path().join("settings.json")).unwrap();

    reload(&mut app);

    assert_eq!(*app.state().theme(), Theme::default());
}

#[test]
fn the_startup_choice_is_kept_by_the_app() {
    let dir = tempfile::tempdir().unwrap();
    let choice = ThemeChoice {
        variant: ThemeVariant::SolLight,
        installed: None,
    };
    let mut app = app_in(dir.path()).with_settings(Settings::default(), choice);
    // A reload with no settings file leaves the theme the settings named at
    // startup, as the setting did not change.
    reload(&mut app);

    assert_eq!(
        app.state().theme().style(SLOT),
        Theme::built_in(ThemeVariant::SolLight).style(SLOT)
    );
}

// ---- the file itself

#[test]
fn the_palette_lists_open_settings() {
    assert!(Action::palette_actions().contains(&Action::OpenConfig(ConfigFile::Settings)));
    assert_eq!(
        Action::OpenConfig(ConfigFile::Settings).title(),
        Some("Config: Open Settings")
    );
}

#[test]
fn open_settings_makes_the_file_from_the_default_and_opens_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_in(dir.path());

    run_from_palette(&mut app, "Open Settings");

    let text = fs::read_to_string(dir.path().join("settings.json")).unwrap();
    assert_eq!(text, DEFAULT_SETTINGS_JSONC);
    assert_eq!(app.state().editor().buffer().text().to_string(), text);
}

#[test]
fn a_new_settings_file_changes_nothing_when_saved() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = started_with(dir.path(), "{ \"theme\": \"light\" }");
    run_from_palette(&mut app, "Theme: SolDark");
    let before = app.state().theme().clone();

    write(dir.path(), "settings.json", "{ \"theme\": \"light\" }");
    reload(&mut app);

    assert_eq!(*app.state().theme(), before);
}

// ---- the hint

#[test]
fn choosing_a_theme_says_it_lasts_for_this_run_and_how_to_keep_it() {
    let mut app = App::default();

    run_from_palette(&mut app, "Theme: Light");

    let message = app.state().latest_notification().unwrap();
    assert!(message.starts_with("theme: light"), "{message}");
    assert!(message.contains("this run only"), "{message}");
    assert!(message.contains("settings.json"), "{message}");
}

#[test]
fn the_slot_the_tests_compare_differs_between_the_built_in_themes() {
    let styles: Vec<_> = ThemeVariant::ALL
        .iter()
        .map(|variant| Theme::built_in(*variant).style(SLOT))
        .collect();

    for (index, style) in styles.iter().enumerate() {
        assert!(
            !styles[index + 1..].contains(style),
            "{:?} has the same {SLOT} as a later theme",
            ThemeVariant::ALL[index]
        );
    }
}
