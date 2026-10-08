use crate::{components::PluginViewId, state::AppState, ui::view::ViewNode};

fn text(line: &str) -> ViewNode {
    ViewNode::List {
        items: vec![line.into()],
        selected: None,
    }
}

#[test]
fn missing_plugin_view_is_none() {
    let state = AppState::default();

    assert!(
        state
            .plugin_view(&PluginViewId::new("debugger.stack"))
            .is_none()
    );
}

#[test]
fn updating_a_plugin_view_bumps_its_version() {
    let mut state = AppState::default();
    let id = PluginViewId::new("debugger.stack");

    state.set_plugin_view(id.clone(), text("a"));
    state.set_plugin_view(id.clone(), text("b"));

    let view = state.plugin_view(&id).unwrap();
    assert_eq!(view.version(), 1);
    assert_eq!(view.content(), &text("b"));
}
