//! The keys of plugins: the ones a plugin declares, which are in force only
//! for the plugin that has the keyboard, and the user's own for plugin panes.

use crate::event::action::Action;
use crate::keymap::{Context, KeyChord, Keymap, KeymapError, Lookup};
use crate::terminal::KeyboardSupport;

const PANE: [Context; 3] = [Context::Plugin, Context::PluginDefaults, Context::Global];

fn found(keymap: &Keymap, keys: &str) -> Option<Action> {
    let chords: Vec<KeyChord> = keys
        .split_whitespace()
        .map(|k| k.parse().unwrap())
        .collect();
    match keymap.find(&PANE, &chords) {
        Lookup::Found { action, .. } => action.cloned(),
        Lookup::NotFound => None,
    }
}

fn plugin(action: &str) -> Action {
    Action::Plugin(action.into())
}

fn bindings(pairs: &[(&str, &str)]) -> Vec<(String, Action)> {
    pairs
        .iter()
        .map(|(keys, action)| ((*keys).to_owned(), plugin(action)))
        .collect()
}

#[test]
fn declared_keys_are_found_in_a_plugin_pane() {
    let mut keymap = Keymap::defaults(KeyboardSupport::Enhanced);

    let errors =
        keymap.set_plugin_defaults(&bindings(&[("s", "git:stage"), ("ctrl+k c", "git:commit")]));

    assert!(errors.is_empty());
    assert_eq!(found(&keymap, "s"), Some(plugin("git:stage")));
    assert_eq!(found(&keymap, "ctrl+k c"), Some(plugin("git:commit")));
}

#[test]
fn the_next_plugins_keys_replace_the_ones_before() {
    let mut keymap = Keymap::defaults(KeyboardSupport::Enhanced);
    keymap.set_plugin_defaults(&bindings(&[("s", "git:stage")]));

    keymap.set_plugin_defaults(&bindings(&[("d", "debug:step")]));

    assert_eq!(found(&keymap, "s"), None);
    assert_eq!(found(&keymap, "d"), Some(plugin("debug:step")));
}

#[test]
fn declared_keys_are_not_in_force_outside_a_plugin_pane() {
    let mut keymap = Keymap::defaults(KeyboardSupport::Enhanced);
    keymap.set_plugin_defaults(&bindings(&[("s", "git:stage")]));

    let editor = [Context::Editor, Context::Global];
    let chords: Vec<KeyChord> = vec!["s".parse().unwrap()];

    assert_eq!(keymap.find(&editor, &chords), Lookup::NotFound);
}

#[test]
fn a_binding_of_the_user_wins_over_a_declared_key() {
    let mut keymap = Keymap::defaults(KeyboardSupport::Enhanced);
    keymap
        .apply_toml(
            r#"[[binding]]
keys = "s"
context = "plugin"
action = "focus_editor""#,
        )
        .unwrap();

    keymap.set_plugin_defaults(&bindings(&[("s", "git:stage")]));

    assert_eq!(found(&keymap, "s"), Some(Action::FocusEditor));
}

#[test]
fn esc_leaves_a_plugin_pane_by_default() {
    let keymap = Keymap::defaults(KeyboardSupport::Enhanced);

    assert_eq!(found(&keymap, "esc"), Some(Action::FocusEditor));
}

#[test]
fn keys_that_cannot_be_read_and_keys_given_twice_come_back_as_errors() {
    let mut keymap = Keymap::defaults(KeyboardSupport::Enhanced);

    let errors = keymap.set_plugin_defaults(&bindings(&[
        ("s", "git:stage"),
        ("s", "git:other"),
        ("ctrl+nonsense+key", "git:bad"),
        ("", "git:empty"),
    ]));

    assert_eq!(errors.len(), 3, "{errors:?}");
    assert!(matches!(errors[0], KeymapError::DuplicateBinding(_)));
    assert!(matches!(errors[1], KeymapError::InvalidKey { .. }));
    assert_eq!(
        found(&keymap, "s"),
        Some(plugin("git:stage")),
        "the first of two stays"
    );
}

#[test]
fn a_plugin_action_can_be_bound_in_a_keymap_file() {
    let mut keymap = Keymap::default();

    keymap
        .apply_toml(
            r#"[[binding]]
keys = "ctrl+g"
action = { plugin = "git:stage" }

[[binding]]
keys = "x"
context = "plugin"
action = { focus_plugin_view = "git" }"#,
        )
        .unwrap();

    assert_eq!(found(&keymap, "ctrl+g"), Some(plugin("git:stage")));
    assert_eq!(
        found(&keymap, "x"),
        Some(Action::FocusPluginView("git".into()))
    );
}

#[test]
fn the_context_of_declared_keys_cannot_be_written_in_a_keymap_file() {
    let mut keymap = Keymap::default();

    let result = keymap.apply_toml(
        r#"[[binding]]
keys = "s"
context = "plugin_defaults"
action = "save""#,
    );

    assert!(matches!(result, Err(KeymapError::Parse(_))));
}
