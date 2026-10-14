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

// ---- tab_size

/// The column the `Z` of a pasted tab and a `Z` shows in, on the first line of the text.
fn z_column(app: &mut App) -> usize {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    terminal
        .draw(|frame| crate::components::prepare_and_render(frame, app.state_mut()))
        .unwrap();
    let screen = terminal.backend().to_string();
    let line = screen
        .lines()
        .find(|line| line.contains('Z'))
        .unwrap_or_else(|| panic!("no Z on the screen:\n{screen}"));
    line.chars().position(|c| c == 'Z').unwrap()
}

fn paste(app: &mut App, text: &str) {
    app.handle_input(InputEvent::Paste(text.to_owned()));
}

#[test]
fn a_tab_draws_as_wide_as_the_setting_says() {
    let dir = temp();
    let mut narrow = started_with(dir.path(), r#"{ "tab_size": 2 }"#);
    let mut wide = started_with(dir.path(), r#"{ "tab_size": 8 }"#);
    paste(&mut narrow, "\tZ");
    paste(&mut wide, "\tZ");

    assert_eq!(z_column(&mut wide) - z_column(&mut narrow), 6);
}

#[test]
fn saving_the_tab_size_redraws_open_tabs_and_applies_to_new_ones() {
    let dir = temp();
    let mut app = started_with(dir.path(), "{}");
    paste(&mut app, "\tZ");
    let before = z_column(&mut app);

    write(dir.path(), "settings.json", r#"{ "tab_size": 8 }"#);
    reload(&mut app);
    assert_eq!(z_column(&mut app), before + 4, "the open tab follows");

    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('n'),
        KeyModifiers::CONTROL,
    )));
    paste(&mut app, "\tZ");
    assert_eq!(z_column(&mut app), before + 4, "a new tab starts with it");
}

#[test]
fn a_reload_that_leaves_the_tab_size_alone_keeps_it_and_a_bad_value_changes_nothing() {
    let dir = temp();
    let mut app = started_with(dir.path(), r#"{ "tab_size": 8 }"#);
    paste(&mut app, "\tZ");
    let before = z_column(&mut app);

    reload(&mut app);
    assert_eq!(z_column(&mut app), before);

    write(dir.path(), "settings.json", r#"{ "tab_size": 99 }"#);
    reload(&mut app);
    assert_eq!(z_column(&mut app), before);
    assert!(notified(&app, "tab_size"));
}

#[test]
fn the_template_names_the_tab_size() {
    assert!(DEFAULT_SETTINGS_JSONC.contains("\"tab_size\""));
}

// ---- indentation

fn typed_tab(app: &mut App) -> String {
    press(app, KeyCode::Tab);
    app.state().editor().buffer().text()
}

#[test]
fn the_tab_key_inserts_as_many_spaces_as_the_tab_size() {
    let dir = temp();

    assert_eq!(
        typed_tab(&mut started_with(dir.path(), "{}")),
        "    ",
        "the default"
    );
    assert_eq!(
        typed_tab(&mut started_with(dir.path(), r#"{ "tab_size": 2 }"#)),
        "  "
    );
    assert_eq!(
        typed_tab(&mut started_with(dir.path(), r#"{ "tab_size": 8 }"#)),
        "        "
    );
}

#[test]
fn the_indent_style_tabs_makes_the_tab_key_insert_a_tab_character() {
    let dir = temp();

    let mut app = started_with(dir.path(), r#"{ "indent_style": "tabs" }"#);

    assert_eq!(typed_tab(&mut app), "\t");
}

#[test]
fn saving_new_indent_settings_applies_to_open_documents_without_a_style_of_their_own() {
    let dir = temp();
    let mut app = started_with(dir.path(), "{}");

    write(dir.path(), "settings.json", r#"{ "tab_size": 2 }"#);
    reload(&mut app);

    assert_eq!(typed_tab(&mut app), "  ");
}

#[test]
fn with_detection_off_the_settings_apply_to_every_document() {
    let dir = temp();
    let file = dir.path().join("t.txt");
    fs::write(&file, "a\n  b\n").unwrap();
    write(
        dir.path(),
        "settings.json",
        r#"{ "tab_size": 8, "detect_indent": false }"#,
    );
    let mut state = crate::initial_state(std::slice::from_ref(&file)).unwrap();
    let user = startup::load_user_config(&mut state, Some(dir.path()), KeyboardSupport::Enhanced);
    let mut app = App::with_keymap(state, user.keymap)
        .with_config_dir(Some(dir.path().to_path_buf()))
        .with_settings(user.settings, user.theme);

    press(&mut app, KeyCode::Tab);

    assert_eq!(
        app.state().editor().buffer().text(),
        "        a\n  b\n",
        "eight spaces, not the file's two"
    );
}

#[test]
fn with_detection_on_a_file_opened_with_two_space_indentation_keeps_two() {
    let dir = temp();
    let file = dir.path().join("t.txt");
    fs::write(&file, "a\n  b\n").unwrap();
    write(dir.path(), "settings.json", r#"{ "tab_size": 8 }"#);
    let mut state = crate::initial_state(std::slice::from_ref(&file)).unwrap();
    let user = startup::load_user_config(&mut state, Some(dir.path()), KeyboardSupport::Enhanced);
    let mut app = App::with_keymap(state, user.keymap)
        .with_config_dir(Some(dir.path().to_path_buf()))
        .with_settings(user.settings, user.theme);

    press(&mut app, KeyCode::Tab);

    assert_eq!(app.state().editor().buffer().text(), "  a\n  b\n");
}

#[test]
fn the_template_names_the_indent_settings() {
    assert!(DEFAULT_SETTINGS_JSONC.contains("\"indent_style\""));
    assert!(DEFAULT_SETTINGS_JSONC.contains("\"detect_indent\""));
}

#[test]
fn saving_a_bigger_tab_size_leaves_an_open_document_with_its_own_indentation_alone() {
    let dir = temp();
    let file = dir.path().join("t.txt");
    fs::write(&file, "a\n  b\n").unwrap();
    write(dir.path(), "settings.json", "{}");
    let mut state = crate::initial_state(std::slice::from_ref(&file)).unwrap();
    let user = startup::load_user_config(&mut state, Some(dir.path()), KeyboardSupport::Enhanced);
    let mut app = App::with_keymap(state, user.keymap)
        .with_config_dir(Some(dir.path().to_path_buf()))
        .with_settings(user.settings, user.theme);

    write(dir.path(), "settings.json", r#"{ "tab_size": 8 }"#);
    reload(&mut app);
    press(&mut app, KeyCode::Tab);

    assert_eq!(app.state().editor().buffer().text(), "  a\n  b\n");
}

// ---- sequence timeout

const SEQUENCE_KEYMAP: &str = r#"
    [[binding]]
    keys = "ctrl+k ctrl+i"
    action = { insert_text = "X" }
"#;

fn started_with_sequence(dir: &Path, settings_text: &str) -> App {
    write(dir, "keymap.toml", SEQUENCE_KEYMAP);
    started_with(dir, settings_text)
}

fn begin_sequence(app: &mut App) {
    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('k'),
        KeyModifiers::CONTROL,
    )));
}

#[test]
fn the_sequence_timeout_setting_sets_how_long_a_sequence_waits() {
    let dir = temp();
    let mut app = started_with_sequence(dir.path(), r#"{ "sequence_timeout": 3000 }"#);
    let before = std::time::Instant::now();

    begin_sequence(&mut app);

    let deadline = app.pending_deadline().expect("a deadline");
    assert!(deadline >= before + std::time::Duration::from_millis(3000));
}

#[test]
fn a_sequence_timeout_of_zero_never_gives_up() {
    let dir = temp();
    let mut app = started_with_sequence(dir.path(), r#"{ "sequence_timeout": 0 }"#);

    begin_sequence(&mut app);

    assert_eq!(app.pending_deadline(), None);
    assert_eq!(app.state().pending_keys(), Some("ctrl+k"));
}

#[test]
fn saving_the_sequence_timeout_applies_it_to_the_next_sequence() {
    let dir = temp();
    let mut app = started_with_sequence(dir.path(), "{}");
    write(dir.path(), "settings.json", r#"{ "sequence_timeout": 0 }"#);
    reload(&mut app);

    begin_sequence(&mut app);

    assert_eq!(app.pending_deadline(), None);
}

#[test]
fn the_default_settings_file_documents_the_sequence_timeout() {
    assert!(DEFAULT_SETTINGS_JSONC.contains("\"sequence_timeout\""));
}

// ---- notification seconds

#[test]
fn the_notification_seconds_setting_sets_how_long_a_message_stays() {
    use std::time::{Duration, Instant};

    let dir = temp();
    let mut app = started_with(dir.path(), r#"{ "notification_seconds": 20 }"#);
    app.state_mut().notify("saved");
    let now = Instant::now();

    app.expire_notifications(now + Duration::from_secs(15));
    assert_eq!(app.state().latest_notification(), Some("saved"));

    app.expire_notifications(now + Duration::from_secs(25));
    assert_eq!(app.state().latest_notification(), None);
}

#[test]
fn an_error_stays_whatever_the_notification_seconds_are() {
    use std::time::{Duration, Instant};

    let dir = temp();
    let mut app = started_with(dir.path(), r#"{ "notification_seconds": 1 }"#);
    app.state_mut().notify_error("save failed");

    app.expire_notifications(Instant::now() + Duration::from_secs(3600));

    assert_eq!(app.state().latest_notification(), Some("save failed"));
}

#[test]
fn saving_the_notification_seconds_applies_them_to_the_next_message() {
    use std::time::{Duration, Instant};

    let dir = temp();
    let mut app = started_with(dir.path(), "{}");
    write(
        dir.path(),
        "settings.json",
        r#"{ "notification_seconds": 30 }"#,
    );
    reload(&mut app);
    app.state_mut().notify("a later message");

    app.expire_notifications(Instant::now() + Duration::from_secs(20));

    assert_eq!(app.state().latest_notification(), Some("a later message"));
}

#[test]
fn the_default_settings_file_documents_the_notification_seconds() {
    assert!(DEFAULT_SETTINGS_JSONC.contains("\"notification_seconds\""));
}

// ---- hide ignored

fn project_with_an_ignored_folder() -> tempfile::TempDir {
    let dir = temp();
    write(dir.path(), ".gitignore", "target/\n");
    write(dir.path(), "kept.txt", "");
    fs::create_dir_all(dir.path().join("target")).unwrap();
    write(dir.path(), "target/out.txt", "");
    dir
}

fn listed(app: &App) -> Vec<String> {
    app.state()
        .explorer()
        .rows()
        .iter()
        .map(|row| row.name.clone())
        .collect()
}

#[test]
fn ignored_entries_are_listed_unless_the_setting_hides_them() {
    let config = temp();
    let project = project_with_an_ignored_folder();
    let mut app = started_with(config.path(), "{}");
    app.state_mut().open_project(project.path()).unwrap();
    assert!(listed(&app).contains(&"target".to_owned()));

    write(
        config.path(),
        "settings.json",
        r#"{ "hide_ignored": true }"#,
    );
    reload(&mut app);

    let rows = listed(&app);
    assert!(!rows.contains(&"target".to_owned()), "{rows:?}");
    assert!(rows.contains(&"kept.txt".to_owned()), "{rows:?}");
}

#[test]
fn the_setting_survives_opening_another_project_and_turning_it_off_lists_them_again() {
    let config = temp();
    let project = project_with_an_ignored_folder();
    let mut app = started_with(config.path(), r#"{ "hide_ignored": true }"#);

    app.state_mut().open_project(project.path()).unwrap();
    assert!(!listed(&app).contains(&"target".to_owned()));

    write(
        config.path(),
        "settings.json",
        r#"{ "hide_ignored": false }"#,
    );
    reload(&mut app);

    assert!(listed(&app).contains(&"target".to_owned()));
}

#[test]
fn the_default_settings_file_documents_hide_ignored() {
    assert!(DEFAULT_SETTINGS_JSONC.contains("\"hide_ignored\""));
}
