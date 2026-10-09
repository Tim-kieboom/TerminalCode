use crate::{app::state::AppState, components::PluginViewId, ui::view::ViewNode};

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

mod popups {
    use std::path::PathBuf;

    use crate::app::state::{AppState, Popup};
    use crate::buffer::{Buffer, Position};
    use crate::components::find::Find;
    use crate::components::finder::Finder;
    use crate::components::palette::Palette;
    use crate::components::quit_prompt::QuitPrompt;
    use crate::components::search::{Options, Search};

    fn quit_prompt() -> QuitPrompt {
        QuitPrompt::new(Vec::new())
    }

    fn palette() -> Palette {
        Palette::new(Vec::new())
    }

    fn finder() -> Finder {
        Finder::new(PathBuf::from("/p"), 1, None)
    }

    fn search() -> Search {
        Search::new(PathBuf::from("/p"), String::new(), Options::default())
    }

    fn find() -> Find {
        Find::open(
            &Buffer::from_text("text"),
            Position::default(),
            String::new(),
            Options::default(),
        )
    }

    fn open_names(state: &AppState) -> Vec<&'static str> {
        let mut open = Vec::new();
        if state.quit_prompt().is_some() {
            open.push("quit_prompt");
        }
        if state.palette().is_some() {
            open.push("palette");
        }
        if state.finder().is_some() {
            open.push("finder");
        }
        if state.search().is_some() {
            open.push("search");
        }
        if state.find().is_some() {
            open.push("find");
        }
        open
    }

    #[test]
    fn no_popup_is_open_at_first() {
        let state = AppState::default();

        assert!(matches!(state.popup(), Popup::None));
        assert!(open_names(&state).is_empty());
    }

    #[test]
    fn opening_a_popup_closes_the_one_that_was_open() {
        let mut state = AppState::default();

        state.open_palette(palette());
        assert_eq!(open_names(&state), ["palette"]);
        state.open_finder(finder());
        assert_eq!(open_names(&state), ["finder"]);
        state.open_search(search());
        assert_eq!(open_names(&state), ["search"]);
        state.open_find(find());
        assert_eq!(open_names(&state), ["find"]);
        state.open_quit_prompt(quit_prompt());
        assert_eq!(open_names(&state), ["quit_prompt"]);
    }

    #[test]
    fn taking_the_open_popup_closes_it_and_hands_it_back() {
        let mut state = AppState::default();
        state.open_palette(palette());

        assert!(state.take_palette().is_some());

        assert!(matches!(state.popup(), Popup::None));
        assert!(state.take_palette().is_none(), "taking twice finds nothing");
    }

    #[test]
    fn taking_a_popup_that_is_not_open_leaves_the_open_one_alone() {
        let mut state = AppState::default();
        state.open_finder(finder());

        assert!(state.take_palette().is_none());
        assert!(state.take_search().is_none());
        assert!(state.take_find().is_none());
        assert!(state.take_quit_prompt().is_none());

        assert_eq!(open_names(&state), ["finder"]);
        assert!(state.take_finder().is_some());
    }

    #[test]
    fn the_open_popup_can_be_changed_in_place() {
        let mut state = AppState::default();
        state.open_search(search());

        state.search_mut().unwrap().insert('x');

        assert_eq!(state.search().unwrap().query(), "x");
        assert!(state.finder_mut().is_none());
    }
}
