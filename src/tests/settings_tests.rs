use std::fs;
use std::path::Path;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::app::state::AppState;
use crate::config::{self, ConfigFile, ConfigWarning, ThemeChoice};
use crate::event::action::Action;
use crate::settings::{DEFAULT_SETTINGS_JSONC, Settings};
use crate::startup;
use crate::terminal::KeyboardSupport;
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

fn temp() -> tempfile::TempDir {
    match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(error) => panic!("no temporary directory: {error}"),
    }
}

fn write(dir: &Path, name: &str, text: &str) {
    if let Err(error) = fs::write(dir.join(name), text) {
        panic!("cannot write {name}: {error}");
    }
}

fn reload(app: &mut App) {
    run_from_palette(app, "Reload");
}

fn app_in(dir: &Path) -> App {
    App::default().with_config_dir(Some(dir.to_path_buf()))
}

/// An app started the way `run` starts one, with the settings file in `dir`.
fn started_with(dir: &Path, settings_text: &str) -> App {
    write(dir, "settings.json", settings_text);
    let mut state = AppState::default();
    let user = startup::load_user_config(&mut state, Some(dir), KeyboardSupport::Enhanced);
    App::with_keymap(state, user.keymap)
        .with_config_dir(Some(dir.to_path_buf()))
        .with_settings(user.settings, user.theme)
}

fn slot(app: &App) -> ratatui::style::Style {
    app.state().theme().style(SLOT)
}

fn slot_of(variant: ThemeVariant) -> ratatui::style::Style {
    Theme::built_in(variant).style(SLOT)
}

fn notified(app: &App, needle: &str) -> bool {
    app.state()
        .notifications()
        .texts()
        .iter()
        .any(|text| text.contains(needle))
}

fn install(dir: &Path, name: &str, text: &str) {
    if let Err(error) = fs::create_dir_all(dir.join("themes")) {
        panic!("cannot make the themes folder: {error}");
    }
    write(&dir.join("themes"), &format!("{name}.toml"), text);
}

// ---- loading

#[test]
fn no_settings_file_is_the_defaults_without_a_warning() {
    let dir = temp();

    for path in [None, Some(dir.path().join("settings.json"))] {
        let loaded = config::load_settings(path.as_deref());

        assert_eq!(loaded.settings, Some(Settings::default()));
        assert!(loaded.warning.is_none());
    }
}

#[test]
fn an_invalid_file_is_reported_with_its_path_and_gives_no_settings() {
    let dir = temp();
    write(dir.path(), "settings.json", "{ \"theme\": 3 }");

    let loaded = config::load_settings(Some(&dir.path().join("settings.json")));

    assert_eq!(loaded.settings, None);
    let warning = loaded.warning.map(|warning| warning.to_string());
    let warning = warning.unwrap_or_default();
    assert!(warning.contains("settings.json"), "{warning}");
    assert!(warning.contains("\"theme\""), "{warning}");
    assert!(warning.ends_with("using default settings"), "{warning}");
}

#[test]
fn a_file_that_cannot_be_read_is_reported_and_gives_no_settings() {
    let dir = temp();
    // A folder where the file should be.
    if let Err(error) = fs::create_dir(dir.path().join("settings.json")) {
        panic!("cannot make the folder: {error}");
    }

    let loaded = config::load_settings(Some(&dir.path().join("settings.json")));

    assert_eq!(loaded.settings, None);
    assert!(matches!(
        loaded.warning,
        Some(ConfigWarning::Unusable { .. })
    ));
}

#[test]
fn an_unknown_setting_is_reported_and_the_rest_applies() {
    let dir = temp();
    write(
        dir.path(),
        "settings.json",
        "{ \"theme\": \"light\", \"colour\": 1 }",
    );

    let loaded = config::load_settings(Some(&dir.path().join("settings.json")));

    assert_eq!(
        loaded.settings.map(|settings| settings.theme).as_deref(),
        Some("light")
    );
    let warning = loaded.warning.map(|warning| warning.to_string());
    assert!(
        warning.is_some_and(|text| text.contains("no such setting: colour")),
        "names the unknown setting"
    );
}

#[test]
fn a_setting_set_twice_is_reported_and_the_last_applies() {
    let dir = temp();
    write(
        dir.path(),
        "settings.json",
        "{ \"theme\": \"light\", \"theme\": \"soldark\" }",
    );

    let loaded = config::load_settings(Some(&dir.path().join("settings.json")));

    assert_eq!(
        loaded.settings.map(|settings| settings.theme).as_deref(),
        Some("soldark")
    );
    let warning = loaded.warning.map(|warning| warning.to_string());
    assert!(
        warning.is_some_and(|text| text.contains("more than once") && text.contains("theme")),
        "names the repeated setting"
    );
}

#[test]
fn an_unknown_and_a_repeated_setting_are_both_reported() {
    let dir = temp();
    write(
        dir.path(),
        "settings.json",
        "{ \"colour\": 1, \"theme\": \"dark\", \"theme\": \"dark\" }",
    );

    let loaded = config::load_settings(Some(&dir.path().join("settings.json")));

    assert!(matches!(loaded.warning, Some(ConfigWarning::Several(_))));
    let text = loaded.warning.map(|w| w.to_string()).unwrap_or_default();
    assert!(
        text.contains("no such setting") && text.contains("more than once"),
        "{text}"
    );
}

// ---- the theme setting

#[test]
fn a_built_in_theme_is_found_by_name_or_title_in_any_case() {
    for name in ["soldark", "SolDark", "SOLDARK"] {
        assert_eq!(
            config::resolve_theme(name, None),
            Some(ThemeChoice::built_in(ThemeVariant::SolDark)),
            "{name}"
        );
    }
}

#[test]
fn an_installed_theme_is_found_by_name() {
    let dir = temp();
    install(dir.path(), "mine", "");

    assert_eq!(
        config::resolve_theme("mine", Some(dir.path())),
        Some(ThemeChoice::installed("mine"))
    );
}

#[test]
fn an_installed_theme_wins_over_a_built_in_one_of_the_same_name() {
    let dir = temp();
    install(dir.path(), "dark", "");

    assert_eq!(
        config::resolve_theme("dark", Some(dir.path())),
        Some(ThemeChoice::installed("dark"))
    );
}

#[test]
fn a_name_that_is_no_theme_finds_nothing() {
    let dir = temp();

    assert_eq!(config::resolve_theme("nope", Some(dir.path())), None);
    assert_eq!(config::resolve_theme("../x", Some(dir.path())), None);
}

#[test]
fn a_theme_choice_names_itself_and_finds_its_file() {
    let dir = temp();

    let built_in = ThemeChoice::built_in(ThemeVariant::Light);
    let installed = ThemeChoice::installed("mine");

    assert_eq!(built_in.name(), "light");
    assert_eq!(installed.name(), "mine");
    assert_eq!(built_in.installed_path(Some(dir.path())), None);
    assert_eq!(
        installed.installed_path(Some(dir.path())),
        Some(dir.path().join("themes").join("mine.toml"))
    );
    assert_eq!(installed.installed_path(None), None);
}

// ---- reload

#[test]
fn the_app_starts_with_the_theme_the_settings_name() {
    let dir = temp();

    let app = started_with(dir.path(), "{ \"theme\": \"light\" }");

    assert_eq!(*app.state().theme(), Theme::built_in(ThemeVariant::Light));
}

#[test]
fn a_reload_that_leaves_the_setting_alone_keeps_the_started_theme() {
    let dir = temp();
    let mut app = started_with(dir.path(), "{ \"theme\": \"light\" }");

    write(dir.path(), "theme.toml", "# touched, but nothing in it\n");
    reload(&mut app);

    assert_eq!(slot(&app), slot_of(ThemeVariant::Light));
}

#[test]
fn saving_the_settings_file_applies_its_theme() {
    let dir = temp();
    let mut app = app_in(dir.path());
    write(dir.path(), "settings.json", "{ \"theme\": \"light\" }");

    reload(&mut app);

    assert_eq!(*app.state().theme(), Theme::built_in(ThemeVariant::Light));
}

#[test]
fn a_theme_chosen_from_the_palette_survives_a_reload_that_leaves_the_setting_alone() {
    let dir = temp();
    let mut app = started_with(dir.path(), "{ \"theme\": \"light\" }");
    run_from_palette(&mut app, "Theme: SolDark");

    write(dir.path(), "theme.toml", "# touched, but nothing in it\n");
    reload(&mut app);

    assert_eq!(slot(&app), slot_of(ThemeVariant::SolDark));
}

#[test]
fn changing_the_setting_again_applies_it_over_a_palette_choice() {
    let dir = temp();
    let mut app = started_with(dir.path(), "{ \"theme\": \"light\" }");
    run_from_palette(&mut app, "Theme: SolDark");

    write(dir.path(), "settings.json", "{ \"theme\": \"sollight\" }");
    reload(&mut app);

    assert_eq!(slot(&app), slot_of(ThemeVariant::SolLight));
}

#[test]
fn an_invalid_settings_file_leaves_the_theme_and_says_so() {
    let dir = temp();
    let mut app = started_with(dir.path(), "{ \"theme\": \"light\" }");
    write(dir.path(), "settings.json", "{ \"theme\": ");

    reload(&mut app);

    assert_eq!(slot(&app), slot_of(ThemeVariant::Light));
    assert!(notified(&app, "using default settings"));
}

#[test]
fn a_theme_name_that_does_not_exist_keeps_the_theme_and_says_so() {
    let dir = temp();
    let mut app = started_with(dir.path(), "{ \"theme\": \"light\" }");
    write(dir.path(), "settings.json", "{ \"theme\": \"nope\" }");

    reload(&mut app);

    assert_eq!(slot(&app), slot_of(ThemeVariant::Light));
    assert!(notified(&app, "no theme called \"nope\""));
}

#[test]
fn a_theme_that_is_made_after_the_setting_named_it_is_applied_at_the_next_reload() {
    let dir = temp();
    let mut app = started_with(dir.path(), "{ \"theme\": \"light\" }");
    write(dir.path(), "settings.json", "{ \"theme\": \"mine\" }");
    reload(&mut app);
    assert_eq!(
        slot(&app),
        slot_of(ThemeVariant::Light),
        "no such theme yet"
    );

    install(dir.path(), "mine", "[palette]\naccent = \"#00ff00\"\n");
    reload(&mut app);

    assert_eq!(slot(&app).fg, Some(ratatui::style::Color::Rgb(0, 255, 0)));
}

#[test]
fn a_removed_settings_file_goes_back_to_the_default_theme() {
    let dir = temp();
    let mut app = started_with(dir.path(), "{ \"theme\": \"light\" }");
    if let Err(error) = fs::remove_file(dir.path().join("settings.json")) {
        panic!("cannot remove the file: {error}");
    }

    reload(&mut app);

    assert_eq!(*app.state().theme(), Theme::default());
}

#[test]
fn saving_the_same_theme_again_changes_nothing() {
    let dir = temp();
    let mut app = started_with(dir.path(), "{ \"theme\": \"light\" }");
    run_from_palette(&mut app, "Theme: SolDark");

    write(dir.path(), "settings.json", "{ \"theme\": \"light\" }");
    reload(&mut app);

    assert_eq!(slot(&app), slot_of(ThemeVariant::SolDark));
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
    let dir = temp();
    let mut app = app_in(dir.path());

    run_from_palette(&mut app, "Open Settings");

    let text = fs::read_to_string(dir.path().join("settings.json")).unwrap_or_default();
    assert_eq!(text, DEFAULT_SETTINGS_JSONC);
    assert_eq!(app.state().editor().buffer().text().to_string(), text);
}

#[test]
fn open_settings_opens_an_existing_file_as_it_is() {
    let dir = temp();
    write(dir.path(), "settings.json", "{ \"theme\": \"light\" }");
    let mut app = app_in(dir.path());

    run_from_palette(&mut app, "Open Settings");

    let text = fs::read_to_string(dir.path().join("settings.json")).unwrap_or_default();
    assert_eq!(text, "{ \"theme\": \"light\" }");
    assert_eq!(app.state().editor().buffer().text().to_string(), text);
}

// ---- the hint

#[test]
fn choosing_a_theme_says_it_lasts_for_this_run_and_how_to_keep_it() {
    let mut app = App::default();

    run_from_palette(&mut app, "Theme: Light");

    let message = app.state().latest_notification().unwrap_or_default();
    assert!(message.starts_with("theme: light"), "{message}");
    assert!(message.contains("this run only"), "{message}");
    assert!(message.contains("settings.json"), "{message}");
}

#[test]
fn the_slot_the_tests_compare_differs_between_the_built_in_themes() {
    let styles: Vec<_> = ThemeVariant::ALL.iter().map(|v| slot_of(*v)).collect();

    for (index, style) in styles.iter().enumerate() {
        assert!(
            !styles[index + 1..].contains(style),
            "{:?} has the same {SLOT} as a later theme",
            ThemeVariant::ALL[index]
        );
    }
}

// ---- shell and mouse

#[test]
fn the_app_starts_with_the_shell_the_settings_name() {
    let dir = temp();

    let app = started_with(dir.path(), r#"{ "shell": "fish" }"#);

    assert_eq!(app.terminal_shell().program, "fish");
}

#[test]
fn without_a_shell_setting_the_users_own_shell_is_used() {
    let dir = temp();

    let app = started_with(dir.path(), "{}");

    assert_eq!(app.terminal_shell(), &crate::pty::Shell::detect());
}

#[test]
fn saving_a_new_shell_applies_it_to_the_next_terminal_tab() {
    let dir = temp();
    let mut app = started_with(dir.path(), "{}");

    write(dir.path(), "settings.json", r#"{ "shell": "zsh" }"#);
    reload(&mut app);

    assert_eq!(app.terminal_shell().program, "zsh");

    write(dir.path(), "settings.json", "{}");
    reload(&mut app);

    assert_eq!(app.terminal_shell(), &crate::pty::Shell::detect());
}

#[test]
fn a_reload_that_leaves_the_shell_setting_alone_keeps_the_shell_in_use() {
    let dir = temp();
    let mut app = started_with(dir.path(), r#"{ "shell": "fish" }"#)
        .with_terminal_shell(crate::pty::Shell::program("something-else"));

    reload(&mut app);

    assert_eq!(app.terminal_shell().program, "something-else");
}

#[test]
fn an_invalid_settings_file_leaves_the_shell_alone() {
    let dir = temp();
    let mut app = started_with(dir.path(), r#"{ "shell": "fish" }"#);

    write(dir.path(), "settings.json", r#"{ "shell": 3 }"#);
    reload(&mut app);

    assert_eq!(app.terminal_shell().program, "fish");
}

#[test]
fn the_mouse_setting_off_tells_the_terminal_at_the_start() {
    let dir = temp();
    let mut app = started_with(dir.path(), r#"{ "mouse": false }"#);

    assert_eq!(app.take_mouse_change(), Some(false));
}

#[test]
fn the_mouse_on_by_default_tells_the_terminal_nothing() {
    let dir = temp();
    let mut app = started_with(dir.path(), "{}");

    assert_eq!(app.take_mouse_change(), None);
}

#[test]
fn saving_the_mouse_setting_applies_it_and_a_reload_that_leaves_it_alone_does_not() {
    let dir = temp();
    let mut app = started_with(dir.path(), "{}");

    write(dir.path(), "settings.json", r#"{ "mouse": false }"#);
    reload(&mut app);
    assert_eq!(app.take_mouse_change(), Some(false));

    // alt+m switches it on for the run; the file still says off.
    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('m'),
        KeyModifiers::ALT,
    )));
    assert_eq!(app.take_mouse_change(), Some(true));
    reload(&mut app);
    assert_eq!(app.take_mouse_change(), None, "the file did not change");
}

#[test]
fn the_template_names_both_new_settings() {
    assert!(DEFAULT_SETTINGS_JSONC.contains("\"shell\""));
    assert!(DEFAULT_SETTINGS_JSONC.contains("\"mouse\""));
}
