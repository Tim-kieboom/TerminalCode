use std::fs;

use super::{Action, KEYBIND_DEFAULTS, KeyBinding, KeyBindings, PanelContext};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("keybinds_test_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn parse_defaults_has_globals_and_contexts() {
    let bindings = KeyBindings::parse_json(KEYBIND_DEFAULTS).unwrap();
    let quit = bindings.get(&Action::Quit, PanelContext::Editor).unwrap();
    assert_eq!(quit.to_string(), "Ctrl+Q");
    let save = bindings.get(&Action::Save, PanelContext::Editor).unwrap();
    assert_eq!(save.to_string(), "Ctrl+S");
}

#[test]
fn parse_invalid_json_is_err() {
    assert!(KeyBindings::parse_json("not json").is_err());
}

#[test]
fn parse_invalid_action_name_is_err() {
    let json = r#"{ "Bogus": "Ctrl+B" }"#;
    assert!(KeyBindings::parse_json(json).is_err());
}

#[test]
fn parse_non_object_context_value_is_err() {
    let json = r#"{ "Editor": ["Ctrl+X"] }"#;
    assert!(KeyBindings::parse_json(json).is_err());
}

#[test]
fn resolve_plain_char_is_insert_char() {
    let bindings = KeyBindings::parse_json(KEYBIND_DEFAULTS).unwrap();
    let action = bindings
        .resolve(
            &key(KeyCode::Char('x'), KeyModifiers::NONE),
            PanelContext::Editor,
        )
        .unwrap();
    assert_eq!(action, Action::InsertChar);

    let action = bindings
        .resolve(
            &key(KeyCode::Char('A'), KeyModifiers::SHIFT),
            PanelContext::Editor,
        )
        .unwrap();
    assert_eq!(action, Action::InsertChar);
}

#[test]
fn resolve_global_binding_in_any_context() {
    let bindings = KeyBindings::parse_json(KEYBIND_DEFAULTS).unwrap();
    let action = bindings
        .resolve(
            &key(KeyCode::Char('q'), KeyModifiers::CONTROL),
            PanelContext::Editor,
        )
        .unwrap();
    assert_eq!(action, Action::Quit);
}

#[test]
fn resolve_prefers_context_over_global() {
    let bindings = KeyBindings::parse_json(KEYBIND_DEFAULTS).unwrap();
    let action = bindings
        .resolve(
            &key(KeyCode::Char('s'), KeyModifiers::CONTROL),
            PanelContext::Editor,
        )
        .unwrap();
    assert_eq!(action, Action::Save);

    let action = bindings.resolve(
        &key(KeyCode::Char('s'), KeyModifiers::CONTROL),
        PanelContext::SideBar,
    );
    assert_eq!(action, None);
}

#[test]
fn resolve_context_binding_without_global() {
    let bindings = KeyBindings::parse_json(KEYBIND_DEFAULTS).unwrap();
    let action = bindings
        .resolve(
            &key(KeyCode::Enter, KeyModifiers::NONE),
            PanelContext::SideBar,
        )
        .unwrap();
    assert_eq!(action, Action::OpenFile);
}

#[test]
fn resolve_ctrl_arrow_keys_scroll_by_word() {
    let bindings = KeyBindings::parse_json(KEYBIND_DEFAULTS).unwrap();
    let action = bindings
        .resolve(
            &key(KeyCode::Left, KeyModifiers::CONTROL),
            PanelContext::Editor,
        )
        .unwrap();
    assert_eq!(action, Action::ScrollWordLeft);

    let action = bindings
        .resolve(
            &key(KeyCode::Right, KeyModifiers::CONTROL),
            PanelContext::Editor,
        )
        .unwrap();
    assert_eq!(action, Action::ScrollWordRight);
}

#[test]
fn resolve_unbound_key_is_none() {
    let bindings = KeyBindings::parse_json(KEYBIND_DEFAULTS).unwrap();
    let action = bindings.resolve(
        &key(KeyCode::Char('p'), KeyModifiers::CONTROL),
        PanelContext::Editor,
    );
    assert_eq!(action, None);
}

#[test]
fn resolve_global_returns_global_bindings_only() {
    let bindings = KeyBindings::parse_json(KEYBIND_DEFAULTS).unwrap();

    let action = bindings
        .resolve_global(&key(KeyCode::Char('q'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(action, Action::Quit);

    let action = bindings
        .resolve_global(&key(KeyCode::Char('b'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(action, Action::ToggleSidebar);

    assert_eq!(
        bindings.resolve_global(&key(KeyCode::Char('s'), KeyModifiers::CONTROL)),
        None
    );
}

#[test]
fn resolve_context_returns_bottombar_scroll_bindings() {
    let bindings = KeyBindings::parse_json(KEYBIND_DEFAULTS).unwrap();

    let action = bindings
        .resolve_context(
            &key(KeyCode::Up, KeyModifiers::CONTROL),
            PanelContext::BottomBar,
        )
        .unwrap();
    assert_eq!(action, Action::ScrollUp);

    let action = bindings
        .resolve_context(
            &key(KeyCode::Down, KeyModifiers::CONTROL),
            PanelContext::BottomBar,
        )
        .unwrap();
    assert_eq!(action, Action::ScrollDown);
}

#[test]
fn resolve_context_ignores_global_and_plain_keys() {
    let bindings = KeyBindings::parse_json(KEYBIND_DEFAULTS).unwrap();

    assert_eq!(
        bindings.resolve_context(
            &key(KeyCode::Up, KeyModifiers::NONE),
            PanelContext::BottomBar,
        ),
        None
    );
    assert_eq!(
        bindings.resolve_context(
            &key(KeyCode::Char('q'), KeyModifiers::CONTROL),
            PanelContext::BottomBar,
        ),
        None
    );
}

#[test]
fn resolve_global_ignores_plain_chars_and_contexts() {
    let bindings = KeyBindings::parse_json(KEYBIND_DEFAULTS).unwrap();

    assert_eq!(
        bindings.resolve_global(&key(KeyCode::Char('x'), KeyModifiers::NONE)),
        None
    );

    assert_eq!(
        bindings.resolve_global(&key(KeyCode::Enter, KeyModifiers::NONE)),
        None
    );
}

#[test]
fn rebind_replaces_binding_for_action() {
    let mut bindings = KeyBindings::parse_json(KEYBIND_DEFAULTS).unwrap();
    bindings.rebind(Action::Quit, KeyBinding::parse("Ctrl+X").unwrap(), None);

    let quit = bindings.get(&Action::Quit, PanelContext::Editor).unwrap();
    assert_eq!(quit.to_string(), "Ctrl+X");
    let action = bindings.resolve(
        &key(KeyCode::Char('q'), KeyModifiers::CONTROL),
        PanelContext::Editor,
    );
    assert_eq!(action, None);
}

#[test]
fn rebind_removes_duplicate_keybinding() {
    let mut bindings = KeyBindings::parse_json(KEYBIND_DEFAULTS).unwrap();
    bindings.rebind(
        Action::ShowKeyBinds,
        KeyBinding::parse("Ctrl+Q").unwrap(),
        None,
    );

    let action = bindings
        .resolve(
            &key(KeyCode::Char('q'), KeyModifiers::CONTROL),
            PanelContext::Editor,
        )
        .unwrap();
    assert_eq!(action, Action::ShowKeyBinds);
}

#[test]
fn rebind_in_context_keeps_global() {
    let mut bindings = KeyBindings::parse_json(KEYBIND_DEFAULTS).unwrap();
    bindings.rebind(
        Action::Save,
        KeyBinding::parse("Ctrl+X").unwrap(),
        Some(PanelContext::Editor),
    );

    let global_save = bindings.get(&Action::Save, PanelContext::SideBar);
    assert_eq!(global_save, None);
    let editor_save = bindings.get(&Action::Save, PanelContext::Editor).unwrap();
    assert_eq!(editor_save.to_string(), "Ctrl+X");
}

#[test]
fn save_writes_loadable_config() {
    let dir = temp_dir("roundtrip");
    let mut bindings = KeyBindings::parse_json(KEYBIND_DEFAULTS).unwrap();
    bindings.rebind(Action::Quit, KeyBinding::parse("Ctrl+X").unwrap(), None);
    bindings.save(&dir).unwrap();

    let loaded = KeyBindings::load(&dir).unwrap();
    let quit = loaded.get(&Action::Quit, PanelContext::Editor).unwrap();
    assert_eq!(quit.to_string(), "Ctrl+X");
    let scroll = loaded.get(&Action::ScrollUp, PanelContext::Editor).unwrap();
    assert_eq!(scroll.to_string(), "Up");
    let save = loaded.get(&Action::Save, PanelContext::Editor).unwrap();
    assert_eq!(save.to_string(), "Ctrl+S");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn load_uses_user_config_over_defaults() {
    let dir = temp_dir("override");
    fs::write(dir.join("keybindings.json"), r#"{ "Quit": "Ctrl+X" }"#).unwrap();

    let bindings = KeyBindings::load(&dir).unwrap();
    let quit = bindings.get(&Action::Quit, PanelContext::Editor).unwrap();
    assert_eq!(quit.to_string(), "Ctrl+X");
    assert_eq!(bindings.get(&Action::Test, PanelContext::Editor), None);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn load_uses_defaults_when_no_config() {
    let dir = temp_dir("defaults");
    let bindings = KeyBindings::load(&dir).unwrap();
    let quit = bindings.get(&Action::Quit, PanelContext::Editor).unwrap();
    assert_eq!(quit.to_string(), "Ctrl+Q");

    let _ = fs::remove_dir_all(&dir);
}
