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

// ---- terminal background lookup

use crate::ui::theme::{Rgb, Theme};

fn state_with_theme(toml: &str) -> AppState {
    let mut state = AppState::default();
    state.set_theme(Theme::from_toml(toml).unwrap());
    state
}

#[test]
fn the_terminal_is_only_asked_when_the_theme_blends_with_it() {
    let mut asked = false;
    let mut solid = state_with_theme("[background]\ncolor = \"#102030\"\n");
    solid.learn_terminal_background(|| {
        asked = true;
        None
    });
    assert!(!asked);

    let mut transparent = state_with_theme("");
    transparent.learn_terminal_background(|| {
        asked = true;
        None
    });
    assert!(!asked);

    let mut tinted = state_with_theme("[background]\ncolor = \"#ffffff\"\nopacity = 0.5\n");
    tinted.learn_terminal_background(|| {
        asked = true;
        Some(Rgb { r: 0, g: 0, b: 0 })
    });
    assert!(asked);
}

#[test]
fn the_answer_is_used_for_the_blend() {
    let mut state = state_with_theme("[background]\ncolor = \"#ffffff\"\nopacity = 0.5\n");

    state.learn_terminal_background(|| Some(Rgb { r: 0, g: 0, b: 0 }));

    assert_eq!(
        state.theme().background_color(),
        Some(ratatui::style::Color::Rgb(128, 128, 128))
    );
}

#[test]
fn a_terminal_that_does_not_answer_leaves_the_tint_color_alone() {
    let mut state = state_with_theme("[background]\ncolor = \"#336699\"\nopacity = 0.5\n");

    state.learn_terminal_background(|| None);

    assert_eq!(
        state.theme().background_color(),
        Some(ratatui::style::Color::Rgb(0x33, 0x66, 0x99))
    );
}
