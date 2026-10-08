use crate::event::action::Action;
use crate::keymap::{Context, KeyChord, Keymap, KeymapError, Lookup};
use crate::terminal::KeyboardSupport;

const BOTH: [Context; 2] = [Context::Editor, Context::Global];

fn chord(source: &str) -> KeyChord {
    source.parse().unwrap()
}

fn sequence(source: &str) -> Vec<KeyChord> {
    source.split_whitespace().map(chord).collect()
}

/// The action bound to exactly `keys` in `contexts`.
fn action(keymap: &Keymap, contexts: &[Context], keys: &str) -> Option<Action> {
    match keymap.find(contexts, &sequence(keys)) {
        Lookup::Found { action, .. } => action.cloned(),
        Lookup::NotFound => None,
    }
}

fn defaults(keyboard: KeyboardSupport) -> Keymap {
    Keymap::defaults(keyboard)
}

#[test]
fn default_keymap_binds_ctrl_q_to_quit_everywhere() {
    let keymap = defaults(KeyboardSupport::Enhanced);

    assert_eq!(
        action(&keymap, &[Context::Global], "ctrl+q"),
        Some(Action::Quit)
    );
    assert_eq!(action(&keymap, &BOTH, "ctrl+q"), Some(Action::Quit));
}

#[test]
fn editing_bindings_only_apply_in_the_editor_context() {
    let keymap = defaults(KeyboardSupport::Enhanced);

    assert_eq!(action(&keymap, &BOTH, "ctrl+s"), Some(Action::Save));
    assert_eq!(action(&keymap, &[Context::Global], "ctrl+s"), None);
}

#[test]
fn unbound_chord_is_not_found() {
    let keymap = defaults(KeyboardSupport::Enhanced);

    assert_eq!(keymap.find(&BOTH, &sequence("ctrl+l")), Lookup::NotFound);
}

#[test]
fn legacy_terminals_get_extra_bindings_enhanced_ones_do_not() {
    let legacy = defaults(KeyboardSupport::Legacy);
    let enhanced = defaults(KeyboardSupport::Enhanced);

    assert_eq!(
        action(&legacy, &BOTH, "ctrl+h"),
        Some(Action::DeleteWordBackward)
    );
    assert_eq!(action(&enhanced, &BOTH, "ctrl+h"), None);
}

#[test]
fn every_default_binding_is_reachable_on_both_keyboards() {
    for keyboard in [KeyboardSupport::Enhanced, KeyboardSupport::Legacy] {
        let keymap = defaults(keyboard);

        assert_eq!(action(&keymap, &BOTH, "ctrl+z"), Some(Action::Undo));
        assert_eq!(
            action(&keymap, &BOTH, "alt+backspace"),
            Some(Action::DeleteWordBackward)
        );
    }
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
        action(&keymap, &BOTH, "alt+d"),
        Some(Action::Plugin("debugger.toggle".into()))
    );
}

#[test]
fn empty_keymap_is_valid() {
    assert!(Keymap::from_toml("").is_ok());
}

#[test]
fn sequences_are_prefixes_until_complete() {
    let source = r#"
        [[binding]]
        keys = "ctrl+k ctrl+s"
        action = "save"
    "#;
    let keymap = Keymap::from_toml(source).unwrap();

    assert_eq!(
        keymap.find(&BOTH, &sequence("ctrl+k")),
        Lookup::Found {
            action: None,
            has_more: true
        }
    );
    assert_eq!(
        keymap.find(&BOTH, &sequence("ctrl+k ctrl+s")),
        Lookup::Found {
            action: Some(&Action::Save),
            has_more: false
        }
    );
    assert_eq!(
        keymap.find(&BOTH, &sequence("ctrl+k ctrl+x")),
        Lookup::NotFound
    );
}

#[test]
fn a_binding_can_also_be_a_prefix() {
    let source = r#"
        [[binding]]
        keys = "ctrl+k"
        action = "quit"

        [[binding]]
        keys = "ctrl+k ctrl+s"
        action = "save"
    "#;
    let keymap = Keymap::from_toml(source).unwrap();

    assert_eq!(
        keymap.find(&BOTH, &sequence("ctrl+k")),
        Lookup::Found {
            action: Some(&Action::Quit),
            has_more: true
        }
    );
}

#[test]
fn user_bindings_replace_defaults_of_the_same_context() {
    let mut keymap = defaults(KeyboardSupport::Enhanced);

    keymap
        .apply_toml(
            r#"
            [[binding]]
            keys = "ctrl+s"
            context = "editor"
            action = "undo"
            "#,
        )
        .unwrap();

    assert_eq!(action(&keymap, &BOTH, "ctrl+s"), Some(Action::Undo));
    assert_eq!(action(&keymap, &BOTH, "ctrl+z"), Some(Action::Undo));
}

#[test]
fn a_binding_without_an_action_removes_the_default() {
    let mut keymap = defaults(KeyboardSupport::Enhanced);

    keymap
        .apply_toml(
            r#"
            [[binding]]
            keys = "ctrl+q"
            "#,
        )
        .unwrap();

    assert_eq!(keymap.find(&BOTH, &sequence("ctrl+q")), Lookup::NotFound);
}

#[test]
fn removing_a_binding_keeps_longer_sequences_that_share_its_prefix() {
    let mut keymap = Keymap::from_toml(
        r#"
        [[binding]]
        keys = "ctrl+k"
        action = "quit"

        [[binding]]
        keys = "ctrl+k ctrl+s"
        action = "save"
        "#,
    )
    .unwrap();

    keymap
        .apply_toml(
            r#"
            [[binding]]
            keys = "ctrl+k"
            "#,
        )
        .unwrap();

    assert_eq!(
        keymap.find(&BOTH, &sequence("ctrl+k")),
        Lookup::Found {
            action: None,
            has_more: true
        }
    );
    assert_eq!(action(&keymap, &BOTH, "ctrl+k ctrl+s"), Some(Action::Save));
}

#[test]
fn removing_the_last_binding_of_a_branch_prunes_it() {
    let mut keymap = Keymap::from_toml(
        r#"
        [[binding]]
        keys = "ctrl+k ctrl+s"
        action = "save"
        "#,
    )
    .unwrap();

    keymap
        .apply_toml(
            r#"
            [[binding]]
            keys = "ctrl+k ctrl+s"
            "#,
        )
        .unwrap();

    assert_eq!(keymap.find(&BOTH, &sequence("ctrl+k")), Lookup::NotFound);
}

#[test]
fn a_more_specific_context_hides_the_broader_one() {
    let keymap = Keymap::from_toml(
        r#"
        [[binding]]
        keys = "ctrl+e"
        action = "quit"

        [[binding]]
        keys = "ctrl+e"
        context = "editor"
        action = "save"
        "#,
    )
    .unwrap();

    assert_eq!(action(&keymap, &BOTH, "ctrl+e"), Some(Action::Save));
    assert_eq!(
        action(&keymap, &[Context::Global], "ctrl+e"),
        Some(Action::Quit)
    );
}

#[test]
fn same_keys_in_different_contexts_are_not_duplicates() {
    let source = r#"
        [[binding]]
        keys = "ctrl+e"
        action = "quit"

        [[binding]]
        keys = "ctrl+e"
        context = "editor"
        action = "save"
    "#;

    assert!(Keymap::from_toml(source).is_ok());
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
fn blank_keys_are_rejected() {
    let source = r#"
        [[binding]]
        keys = "   "
        action = "quit"
    "#;

    assert!(matches!(
        Keymap::from_toml(source),
        Err(KeymapError::InvalidKey { .. })
    ));
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

#[test]
fn an_invalid_layer_changes_nothing() {
    let mut keymap = defaults(KeyboardSupport::Enhanced);
    let before = keymap.clone();

    let result = keymap.apply_toml(
        r#"
        [[binding]]
        keys = "ctrl+s"
        context = "editor"
        action = "undo"

        [[binding]]
        keys = "ctrl+banana"
        action = "quit"
        "#,
    );

    assert!(result.is_err());
    assert_eq!(keymap, before);
}

#[test]
fn keys_for_prefers_fewer_chords_then_shorter_text() {
    let keymap = Keymap::from_toml(
        r#"
        [[binding]]
        keys = "ctrl+k ctrl+s"
        action = "save"
        [[binding]]
        keys = "ctrl+shift+s"
        action = "save"
        [[binding]]
        keys = "f2"
        action = "save"
        "#,
    )
    .unwrap();

    assert_eq!(keymap.keys_for(&Action::Save, &BOTH).as_deref(), Some("f2"));
}

#[test]
fn keys_for_is_none_for_an_unbound_action_or_a_context_not_asked_for() {
    let keymap = Keymap::from_toml(
        r#"
        [[binding]]
        keys = "ctrl+s"
        context = "editor"
        action = "save"
        "#,
    )
    .unwrap();

    assert_eq!(keymap.keys_for(&Action::Undo, &BOTH), None);
    assert_eq!(keymap.keys_for(&Action::Save, &[Context::Global]), None);
    assert_eq!(
        keymap
            .keys_for(&Action::Save, &[Context::Editor])
            .as_deref(),
        Some("ctrl+s")
    );
}
