use std::fs;
use std::path::Path;

use crate::app::state::AppState;
use crate::config::ThemeChoice;
use crate::keymap::{Context, KeyChord, Lookup};
use crate::settings::Settings;
use crate::startup::*;
use crate::terminal::KeyboardSupport;
use crate::ui::ComponentKind;
use crate::ui::theme::{Theme, ThemeVariant};

fn write(dir: &Path, name: &str, text: &str) {
    if let Err(error) = fs::write(dir.join(name), text) {
        panic!("cannot write {name}: {error}");
    }
}

fn temp() -> tempfile::TempDir {
    match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(error) => panic!("no temporary directory: {error}"),
    }
}

fn load(dir: Option<&Path>) -> (AppState, UserConfig) {
    let mut state = AppState::default();
    let user = load_user_config(&mut state, dir, KeyboardSupport::Enhanced);
    (state, user)
}

/// A theme the user made from the Light template.
fn install_light(dir: &Path) {
    fs::create_dir_all(dir.join("themes")).unwrap();
    fs::write(dir.join("themes/light.toml"), ThemeVariant::Light.source()).unwrap();
}

/// What start-up said that is a problem: the note that settings were added to
/// the user's file is not one.
fn errors(state: &AppState) -> Vec<String> {
    state
        .notifications()
        .texts()
        .into_iter()
        .filter(|text| !text.starts_with("added new settings"))
        .map(str::to_owned)
        .collect()
}

fn has_error(state: &AppState, needle: &str) -> bool {
    errors(state).iter().any(|text| text.contains(needle))
}

#[test]
fn without_a_config_folder_everything_is_the_default_and_nothing_is_said() {
    let (state, user) = load(None);

    assert_eq!(user.settings, Settings::default());
    assert_eq!(user.theme, ThemeChoice::default());
    assert_eq!(*state.theme(), Theme::default());
    assert!(errors(&state).is_empty());
}

#[test]
fn an_empty_config_folder_says_nothing_either() {
    let dir = temp();

    let (state, user) = load(Some(dir.path()));

    assert_eq!(user.settings, Settings::default());
    assert!(errors(&state).is_empty());
}

#[test]
fn the_theme_the_settings_name_is_put_in_the_state_and_returned() {
    let dir = temp();
    install_light(dir.path());
    write(dir.path(), "settings.json", "{ \"theme\": \"light\" }");

    let (state, user) = load(Some(dir.path()));

    assert_eq!(user.theme, ThemeChoice::installed("light"));
    assert_eq!(*state.theme(), Theme::built_in(ThemeVariant::Light));
    assert!(errors(&state).is_empty());
}

#[test]
fn an_installed_theme_named_by_the_settings_is_used() {
    let dir = temp();
    write(dir.path(), "settings.json", "{ \"theme\": \"mine\" }");
    if let Err(error) = fs::create_dir(dir.path().join("themes")) {
        panic!("cannot make the folder: {error}");
    }
    write(
        &dir.path().join("themes"),
        "mine.toml",
        "[palette]\naccent = \"#00ff00\"\n",
    );

    let (state, user) = load(Some(dir.path()));

    assert_eq!(user.theme, ThemeChoice::installed("mine"));
    assert_eq!(
        state.theme().style("pane.border.focused").fg,
        Some(ratatui::style::Color::Rgb(0, 255, 0))
    );
}

#[test]
fn a_theme_name_that_is_no_theme_gives_the_dark_theme_and_an_error() {
    let dir = temp();
    write(dir.path(), "settings.json", "{ \"theme\": \"nope\" }");

    let (state, user) = load(Some(dir.path()));

    assert_eq!(user.theme, ThemeChoice::default());
    assert_eq!(*state.theme(), Theme::default());
    assert!(has_error(&state, "no theme called \"nope\""));
}

#[test]
fn an_invalid_settings_file_gives_the_defaults_and_an_error() {
    let dir = temp();
    write(dir.path(), "settings.json", "{ \"theme\": 3 }");

    let (state, user) = load(Some(dir.path()));

    assert_eq!(user.settings, Settings::default());
    assert!(has_error(&state, "using default settings"));
}

#[test]
fn an_unknown_setting_is_an_error_but_the_rest_of_the_file_applies() {
    let dir = temp();
    install_light(dir.path());
    write(
        dir.path(),
        "settings.json",
        "{ \"colour\": 1, \"theme\": \"light\" }",
    );

    let (state, user) = load(Some(dir.path()));

    assert_eq!(user.theme, ThemeChoice::installed("light"));
    assert!(has_error(&state, "no such setting: colour"));
}

#[test]
fn a_broken_keymap_is_an_error_and_the_other_files_still_apply() {
    let dir = temp();
    install_light(dir.path());
    write(dir.path(), "settings.json", "{ \"theme\": \"light\" }");
    write(dir.path(), "keymap.toml", "this is not a keymap");

    let (state, user) = load(Some(dir.path()));

    assert!(has_error(&state, "using default keybindings"));
    assert_eq!(*state.theme(), Theme::built_in(ThemeVariant::Light));
    let quit: KeyChord = match "ctrl+q".parse() {
        Ok(chord) => chord,
        Err(error) => panic!("ctrl+q should parse: {error}"),
    };
    assert!(matches!(
        user.keymap.find(&[Context::Global], &[quit]),
        Lookup::Found { .. }
    ));
}

#[test]
fn a_broken_layout_is_an_error() {
    let dir = temp();
    write(dir.path(), "layout.ron", "(((");

    let (state, _) = load(Some(dir.path()));

    assert!(has_error(&state, "using default layout"));
}

#[test]
fn a_broken_theme_file_is_an_error_and_the_chosen_theme_stands() {
    let dir = temp();
    install_light(dir.path());
    write(dir.path(), "settings.json", "{ \"theme\": \"light\" }");
    write(dir.path(), "theme.toml", "[palette\n");

    let (state, _) = load(Some(dir.path()));

    assert!(has_error(&state, "theme.toml"));
    assert_eq!(*state.theme(), Theme::built_in(ThemeVariant::Light));
}

#[test]
fn every_problem_is_its_own_error() {
    let dir = temp();
    write(dir.path(), "settings.json", "{ \"theme\": \"nope\" }");
    write(dir.path(), "keymap.toml", "this is not a keymap");
    write(dir.path(), "layout.ron", "(((");

    let (state, _) = load(Some(dir.path()));

    assert_eq!(errors(&state).len(), 3, "{:?}", errors(&state));
}

// ---- what is hidden at the start

const LAYOUT_WITH_HIDDEN_EXPLORER: &str = "Col([
    Row([
        Fixed(20, Pane(view: Explorer, hidden: true)),
        Pane(view: Editor),
    ]),
    Fixed(1, Pane(view: StatusBar)),
])";

fn shown(state: &AppState) -> [bool; 3] {
    [
        ComponentKind::Explorer,
        ComponentKind::Terminal,
        ComponentKind::StatusBar,
    ]
    .map(|kind| state.is_visible(&kind))
}

#[test]
fn without_a_layout_field_or_a_remembered_state_panes_start_as_they_always_did() {
    let dir = temp();

    let (state, _) = load(Some(dir.path()));

    assert_eq!(shown(&state), [true, false, true]);
}

#[test]
fn a_layout_can_start_a_pane_hidden_or_shown() {
    let dir = temp();
    write(dir.path(), "layout.ron", LAYOUT_WITH_HIDDEN_EXPLORER);
    let (state, _) = load(Some(dir.path()));
    assert_eq!(shown(&state), [false, false, true]);

    write(
        dir.path(),
        "layout.ron",
        "Col([Pane(view: Editor), Fixed(8, Pane(view: Terminal, hidden: false))])",
    );
    let (state, _) = load(Some(dir.path()));
    assert!(state.is_visible(&ComponentKind::Terminal));
}

#[test]
fn what_was_remembered_wins_over_the_layout_in_both_directions() {
    let dir = temp();
    write(dir.path(), "layout.ron", LAYOUT_WITH_HIDDEN_EXPLORER);
    write(
        dir.path(),
        "state.json",
        r#"{ "shown": { "explorer": true } }"#,
    );
    let (state, _) = load(Some(dir.path()));
    assert!(state.is_visible(&ComponentKind::Explorer));

    write(
        dir.path(),
        "layout.ron",
        "Col([Pane(view: Explorer), Pane(view: Editor)])",
    );
    write(
        dir.path(),
        "state.json",
        r#"{ "shown": { "explorer": false } }"#,
    );
    let (state, _) = load(Some(dir.path()));
    assert!(!state.is_visible(&ComponentKind::Explorer));
}

#[test]
fn a_pane_with_nothing_remembered_follows_the_layout_while_another_is_remembered() {
    let dir = temp();
    write(dir.path(), "layout.ron", LAYOUT_WITH_HIDDEN_EXPLORER);
    write(
        dir.path(),
        "state.json",
        r#"{ "shown": { "status_bar": false } }"#,
    );

    let (state, _) = load(Some(dir.path()));

    assert_eq!(shown(&state), [false, false, false]);
}

#[test]
fn a_state_file_that_is_not_valid_is_reported_and_the_layout_is_used() {
    let dir = temp();
    write(dir.path(), "layout.ron", LAYOUT_WITH_HIDDEN_EXPLORER);
    write(dir.path(), "state.json", "{ nope");

    let (state, _) = load(Some(dir.path()));

    assert!(has_error(&state, "state.json"), "{:?}", errors(&state));
    assert_eq!(shown(&state), [false, false, true]);
}

#[test]
fn a_recent_file_that_is_not_valid_is_reported_and_the_list_starts_empty() {
    let dir = temp();
    write(dir.path(), "recent.json", "{ nope");

    let (state, config) = load(Some(dir.path()));

    assert!(has_error(&state, "recent.json"), "{:?}", errors(&state));
    assert!(config.recent.is_empty());
}

#[test]
fn the_recent_list_is_read_from_its_file() {
    let dir = temp();
    write(
        dir.path(),
        "recent.json",
        r#"{ "files": ["/p/a.rs"], "folders": ["/p"] }"#,
    );

    let (state, config) = load(Some(dir.path()));

    assert!(errors(&state).is_empty());
    assert_eq!(config.recent.files().len(), 1);
    assert_eq!(config.recent.folders().len(), 1);
}

#[test]
fn a_layout_field_with_the_wrong_type_is_a_layout_error() {
    let dir = temp();
    write(
        dir.path(),
        "layout.ron",
        "Col([Pane(view: Explorer, hidden: \"yes\"), Pane(view: Editor)])",
    );

    let (state, _) = load(Some(dir.path()));

    assert!(has_error(&state, "layout"), "{:?}", errors(&state));
}

// ---- the built-in themes are templates, not themes to choose

#[test]
fn no_theme_setting_is_the_fallback_without_a_warning() {
    let dir = temp();
    write(dir.path(), "settings.json", "{ \"theme\": null }");

    let (state, user) = load(Some(dir.path()));

    assert_eq!(user.theme, ThemeChoice::default());
    assert_eq!(*state.theme(), Theme::default());
    assert!(errors(&state).is_empty(), "{:?}", errors(&state));
}

#[test]
fn an_old_built_in_theme_name_with_no_theme_of_that_name_warns_and_uses_the_fallback() {
    for name in ["dark", "light", "soldark", "sollight"] {
        let dir = temp();
        write(
            dir.path(),
            "settings.json",
            &format!("{{ \"theme\": \"{name}\" }}"),
        );

        let (state, user) = load(Some(dir.path()));

        assert_eq!(user.theme, ThemeChoice::default(), "{name}");
        assert!(
            has_error(&state, &format!("no theme called \"{name}\"")),
            "{name}"
        );
        assert!(has_error(&state, "Theme: New"), "{name}");
        assert_eq!(*state.theme(), Theme::default(), "{name}");
    }
}

#[test]
fn a_theme_the_user_made_from_a_template_is_chosen_by_name() {
    let dir = temp();
    install_light(dir.path());
    write(dir.path(), "settings.json", "{ \"theme\": \"light\" }");

    let (state, user) = load(Some(dir.path()));

    assert_eq!(user.theme, ThemeChoice::installed("light"));
    assert!(errors(&state).is_empty());
}

// ---- settings a new version introduced

#[test]
fn start_up_adds_the_new_settings_to_the_users_file_and_remembers_the_version() {
    let dir = temp();
    write(
        dir.path(),
        "settings.json",
        "{\n    // mine\n    \"tab_size\": 2\n}\n",
    );

    let (state, user) = load(Some(dir.path()));

    let text = fs::read_to_string(dir.path().join("settings.json")).unwrap();
    assert!(text.contains("// mine\n    \"tab_size\": 2,\n"), "{text}");
    assert!(text.contains("\"format_on_save\": false,"), "{text}");
    assert!(
        state
            .notifications()
            .texts()
            .contains(&"added new settings to settings.json: format_on_save")
    );
    assert_eq!(user.saved.settings_version(), Some("0.7.0"));
    let remembered = fs::read_to_string(dir.path().join("state.json")).unwrap();
    assert!(
        remembered.contains("\"settings_version\": \"0.7.0\""),
        "{remembered}"
    );
    assert_eq!(user.settings.tab_size.get(), 2);
}

#[test]
fn start_up_with_no_settings_file_makes_none() {
    let dir = temp();

    let (_, user) = load(Some(dir.path()));

    assert!(!dir.path().join("settings.json").exists());
    assert!(!dir.path().join("state.json").exists());
    assert_eq!(user.saved.settings_version(), None);
}

#[test]
fn start_up_with_a_file_that_cannot_be_updated_says_so_and_does_not_remember() {
    let dir = temp();
    let source = "{\n    \"tab_size\": 2\n    // last\n}\n";
    write(dir.path(), "settings.json", source);

    let (state, user) = load(Some(dir.path()));

    assert_eq!(
        fs::read_to_string(dir.path().join("settings.json")).unwrap(),
        source
    );
    assert!(has_error(
        &state,
        "was not updated with the new settings (a comment follows its last entry)"
    ));
    assert!(!dir.path().join("state.json").exists());
    assert_eq!(user.settings.tab_size.get(), 2, "its settings still apply");
}
