use super::{Action, PanelContext};

#[test]
fn all_actions_have_non_empty_descriptions() {
    for action in Action::all() {
        assert!(!action.description().is_empty());
    }
}

#[test]
fn all_actions_round_trip_through_description() {
    for action in Action::all() {
        assert_eq!(
            Action::from_description(action.description()),
            Some(*action)
        );
    }
}

#[test]
fn all_actions_round_trip_through_name() {
    for action in Action::all() {
        assert_eq!(Action::from_name(&format!("{action:?}")), Some(*action));
    }
}

#[test]
fn from_description_is_case_insensitive() {
    assert_eq!(
        Action::from_description("Scroll up"),
        Some(Action::ScrollUp)
    );
    assert_eq!(
        Action::from_description("SCROLL UP"),
        Some(Action::ScrollUp)
    );
    assert_eq!(Action::from_description("Quit"), Some(Action::Quit));
    assert_eq!(
        PanelContext::from_description("DebugWindow"),
        Some(PanelContext::DebugWindow)
    );
    assert_eq!(
        PanelContext::from_description("editor"),
        Some(PanelContext::Editor)
    );
}

#[test]
fn unknown_descriptions_and_names_return_none() {
    assert_eq!(Action::from_description("not an action"), None);
    assert_eq!(Action::from_name("Scroll up"), None);
}

#[test]
fn serde_serializes_actions_as_camel_case() {
    assert_eq!(
        serde_json::to_string(&Action::ShowKeyBinds).unwrap(),
        "\"showKeyBinds\""
    );
    assert_eq!(
        serde_json::to_string(&Action::OpenFile).unwrap(),
        "\"openFile\""
    );
    assert_eq!(
        serde_json::to_string(&PanelContext::DebugWindow).unwrap(),
        "\"debugWindow\""
    );
}
