use crate::action::Action;
use crate::keymap::{KeyChord, Keymap, KeymapError};

fn chord(source: &str) -> KeyChord {
    source.parse().unwrap()
}

#[test]
fn default_keymap_binds_ctrl_q_to_quit() {
    let keymap = Keymap::default();

    assert_eq!(keymap.lookup(&chord("ctrl+q")), Some(&Action::Quit));
}

#[test]
fn unbound_chord_has_no_action() {
    assert_eq!(Keymap::default().lookup(&chord("ctrl+x")), None);
}

#[test]
fn from_toml_reads_plugin_actions() {
    let source = r#"
        [[binding]]
        keys = "alt+d"
        action = { plugin = "debugger.toggle" }
    "#;

    let keymap = Keymap::from_toml(source).unwrap();

    assert_eq!(
        keymap.lookup(&chord("alt+d")),
        Some(&Action::Plugin("debugger.toggle".into()))
    );
}

#[test]
fn empty_keymap_is_valid() {
    assert!(Keymap::from_toml("").is_ok());
}

#[test]
fn bindings_with_equivalent_spellings_are_duplicates() {
    let source = r#"
        [[binding]]
        keys = "ctrl+shift+p"
        action = "quit"

        [[binding]]
        keys = "Shift+Ctrl+P"
        action = "quit"
    "#;

    let error = Keymap::from_toml(source).unwrap_err();

    assert!(matches!(error, KeymapError::DuplicateBinding(keys) if &*keys == "Shift+Ctrl+P"));
}

#[test]
fn invalid_key_error_names_the_binding() {
    let source = r#"
        [[binding]]
        keys = "ctrl+banana"
        action = "quit"
    "#;

    let error = Keymap::from_toml(source).unwrap_err();

    assert!(matches!(error, KeymapError::InvalidKey { .. }));
    assert!(error.to_string().contains("ctrl+banana"));
}

#[test]
fn unknown_action_is_a_parse_error() {
    let source = r#"
        [[binding]]
        keys = "ctrl+k"
        action = "explode"
    "#;

    assert!(matches!(
        Keymap::from_toml(source),
        Err(KeymapError::Parse(_))
    ));
}
