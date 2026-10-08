use std::fs;

use crate::action::Action;
use crate::config::load_keymap;
use crate::keymap::{Context, KeyChord, Keymap, Lookup};
use crate::terminal::KeyboardSupport;

fn bound(keymap: &Keymap, keys: &str) -> Option<Action> {
    let chords: Vec<KeyChord> = keys
        .split_whitespace()
        .map(|k| k.parse().unwrap())
        .collect();
    match keymap.find(&[Context::Editor, Context::Global], &chords) {
        Lookup::Found { action, .. } => action.cloned(),
        Lookup::NotFound => None,
    }
}

#[test]
fn no_user_file_gives_the_defaults_without_a_warning() {
    let loaded = load_keymap(None, KeyboardSupport::Enhanced);

    assert!(loaded.warning.is_none());
    assert_eq!(bound(&loaded.keymap, "ctrl+q"), Some(Action::Quit));
}

#[test]
fn a_missing_user_file_is_not_an_error() {
    let dir = tempfile::tempdir().unwrap();

    let loaded = load_keymap(
        Some(&dir.path().join("keymap.toml")),
        KeyboardSupport::Enhanced,
    );

    assert!(loaded.warning.is_none());
}

#[test]
fn user_file_is_layered_over_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keymap.toml");
    fs::write(
        &path,
        r#"
        [[binding]]
        keys = "ctrl+s"
        context = "editor"
        action = "undo"

        [[binding]]
        keys = "ctrl+q"
        "#,
    )
    .unwrap();

    let loaded = load_keymap(Some(&path), KeyboardSupport::Enhanced);

    assert!(loaded.warning.is_none());
    assert_eq!(bound(&loaded.keymap, "ctrl+s"), Some(Action::Undo));
    assert_eq!(bound(&loaded.keymap, "ctrl+q"), None);
    assert_eq!(bound(&loaded.keymap, "ctrl+z"), Some(Action::Undo));
}

#[test]
fn an_invalid_user_file_falls_back_to_the_defaults_with_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keymap.toml");
    fs::write(
        &path,
        "[[binding]]\nkeys = \"ctrl+banana\"\naction = \"quit\"\n",
    )
    .unwrap();

    let loaded = load_keymap(Some(&path), KeyboardSupport::Enhanced);

    let warning = loaded.warning.unwrap();
    assert!(warning.contains("keymap.toml"), "{warning}");
    assert!(warning.contains("ctrl+banana"), "{warning}");
    assert_eq!(bound(&loaded.keymap, "ctrl+q"), Some(Action::Quit));
}

#[test]
fn an_unreadable_user_file_falls_back_with_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    // A directory where the file should be: reading it fails with a non-NotFound error.
    let path = dir.path().join("keymap.toml");
    fs::create_dir(&path).unwrap();

    let loaded = load_keymap(Some(&path), KeyboardSupport::Enhanced);

    assert!(loaded.warning.is_some());
    assert_eq!(bound(&loaded.keymap, "ctrl+q"), Some(Action::Quit));
}

#[test]
fn legacy_terminals_get_the_legacy_bindings_in_the_loaded_keymap() {
    let loaded = load_keymap(None, KeyboardSupport::Legacy);

    assert_eq!(
        bound(&loaded.keymap, "ctrl+h"),
        Some(Action::DeleteWordBackward)
    );
}
