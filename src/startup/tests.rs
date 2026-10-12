use std::fs;
use std::path::Path;

use super::*;
use crate::keymap::{Context, KeyChord, Lookup};
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

fn errors(state: &AppState) -> Vec<String> {
    state
        .notifications()
        .texts()
        .into_iter()
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
    write(dir.path(), "settings.json", "{ \"theme\": \"light\" }");

    let (state, user) = load(Some(dir.path()));

    assert_eq!(user.theme, ThemeChoice::built_in(ThemeVariant::Light));
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
    write(
        dir.path(),
        "settings.json",
        "{ \"colour\": 1, \"theme\": \"light\" }",
    );

    let (state, user) = load(Some(dir.path()));

    assert_eq!(user.theme, ThemeChoice::built_in(ThemeVariant::Light));
    assert!(has_error(&state, "no such setting: colour"));
}

#[test]
fn a_broken_keymap_is_an_error_and_the_other_files_still_apply() {
    let dir = temp();
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
