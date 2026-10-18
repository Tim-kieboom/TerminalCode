//! A bar holds several views and shows one: the tab strip, switching by click
//! and by key, and what showing and hiding a bar does to the views in it.

use crossterm::event::{
    Event as InputEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::app::state::{AppState, Focus};
use crate::components::prepare_and_render;
use crate::event::action::Action;
use crate::ui::bar::tab_rects;
use crate::ui::layout::LayoutTree;
use crate::ui::{ComponentKind, PluginViewId};

const LAYOUT: &str = r#"Col([
    Row([
        Fixed(30, SideBar(views: [Explorer, Plugin("git")], sides: [Right])),
        Col([
            Pane(view: Editor),
            Fixed(10, BottomBar(views: [Terminal, Plugin("errors")], sides: [Top])),
            Fixed(1, Pane(view: StatusBar)),
        ]),
    ]),
])"#;

fn plugin(name: &str) -> ComponentKind {
    ComponentKind::Plugin(PluginViewId::new(name))
}

fn app() -> App {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    std::fs::write(dir.path().join("b.txt"), "b").unwrap();
    let mut state = AppState::default();
    state.set_layout(LayoutTree::from_ron(LAYOUT).unwrap());
    state.open_project(dir.path()).unwrap();
    let mut app = App::new(state);
    draw(&mut app);
    app
}

fn draw(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    terminal.backend().to_string()
}

fn click_at(app: &mut App, kind: MouseEventKind, column: u16, row: u16) {
    app.handle_input(InputEvent::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

/// The centre of the tab of `view` in the bar that shows `shown`.
fn tab_of(app: &App, shown: &ComponentKind, view: usize) -> (u16, u16) {
    let placements = app
        .state()
        .placements(ratatui::layout::Rect::new(0, 0, 100, 30));
    let placement = placements.iter().find(|p| p.kind == *shown).unwrap();
    let (rect, _) = tab_rects(placement)
        .into_iter()
        .find(|(_, index)| *index == view)
        .unwrap();
    (rect.x + rect.width / 2, rect.y)
}

#[test]
fn a_bar_with_two_views_draws_both_names_in_its_title_row() {
    let mut app = app();

    let screen = draw(&mut app);

    assert!(screen.contains("Explorer - git"), "{screen}");
}

#[test]
fn a_click_on_the_other_tab_shows_that_view() {
    let mut app = app();
    let (x, y) = tab_of(&app, &ComponentKind::Explorer, 1);

    click_at(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);
    let screen = draw(&mut app);

    assert!(app.state().is_showing(&plugin("git")));
    assert!(!app.state().is_showing(&ComponentKind::Explorer));
    assert_eq!(
        app.state().focus(),
        Focus::Editor,
        "a plugin view has no keyboard"
    );
    assert!(
        screen.contains("Explorer - git"),
        "the strip stays: {screen}"
    );
}

#[test]
fn switching_back_keeps_the_state_of_the_view_that_was_away() {
    let mut app = app();
    let rows = app.state().explorer().rows().len();
    assert!(rows > 0);
    let (x, y) = tab_of(&app, &ComponentKind::Explorer, 1);
    click_at(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);
    draw(&mut app);

    let (x, y) = tab_of(&app, &plugin("git"), 0);
    click_at(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);

    assert!(app.state().is_showing(&ComponentKind::Explorer));
    assert_eq!(app.state().focus(), Focus::Explorer);
    assert_eq!(app.state().explorer().rows().len(), rows);
}

#[test]
fn a_middle_click_on_a_tab_goes_to_the_next_view() {
    let mut app = app();
    let (x, y) = tab_of(&app, &ComponentKind::Explorer, 0);

    click_at(&mut app, MouseEventKind::Down(MouseButton::Middle), x, y);

    assert!(app.state().is_showing(&plugin("git")));
}

#[test]
fn the_next_view_action_cycles_the_bar_the_keyboard_is_in() {
    let mut app = app();
    app.state_mut().set_focus(Focus::Explorer);

    app.do_action(Action::NextBarView);
    assert!(app.state().is_showing(&plugin("git")));
    assert_eq!(app.state().focus(), Focus::Editor);

    app.state_mut().select_view(&ComponentKind::Explorer);
    app.state_mut().set_focus(Focus::Explorer);
    app.do_action(Action::PreviousBarView);
    assert!(app.state().is_showing(&plugin("git")), "wraps round");
}

#[test]
fn the_next_view_action_says_so_when_the_keyboard_is_not_in_a_bar() {
    let mut app = app();

    app.do_action(Action::NextBarView);

    assert!(app.state().is_showing(&ComponentKind::Explorer));
}

#[test]
fn hiding_the_view_a_bar_shows_hides_the_bar_and_showing_any_view_brings_it_back() {
    let mut app = app();
    let size = ratatui::layout::Rect::new(0, 0, 100, 30);
    assert!(
        app.state()
            .placements(size)
            .iter()
            .any(|p| p.kind == ComponentKind::Explorer)
    );

    app.state_mut().hide_explorer();
    let hidden: Vec<_> = app
        .state()
        .placements(size)
        .into_iter()
        .map(|p| p.kind)
        .collect();
    assert!(!hidden.contains(&ComponentKind::Explorer));
    assert!(!hidden.contains(&plugin("git")), "the whole bar is gone");

    app.state_mut().set_shown(&plugin("git"), true);
    let again: Vec<_> = app
        .state()
        .placements(size)
        .into_iter()
        .map(|p| p.kind)
        .collect();
    assert!(again.contains(&plugin("git")));
}

#[test]
fn the_toggle_action_for_a_view_that_is_not_showing_selects_it_and_shows_the_bar() {
    let mut app = app();
    app.state_mut().select_view(&plugin("git"));
    assert!(!app.state().is_showing(&ComponentKind::Explorer));

    app.do_action(Action::ToggleExplorer);

    assert!(app.state().is_showing(&ComponentKind::Explorer));
    assert_eq!(app.state().focus(), Focus::Explorer);
}

#[test]
fn the_two_bars_switch_independently() {
    let mut app = app();

    app.state_mut().select_view(&plugin("errors"));

    assert!(app.state().is_showing(&ComponentKind::Explorer));
    assert!(app.state().is_showing(&plugin("errors")));
    assert!(!app.state().is_showing(&ComponentKind::Terminal));
}

#[test]
fn the_tab_strip_does_not_cover_the_first_row_of_the_explorer_when_the_bar_has_no_top_border() {
    let mut app = app();
    let root_name = app
        .state()
        .explorer()
        .rows()
        .first()
        .map(|row| row.name.clone())
        .expect("the project has a root row");

    let screen = draw(&mut app);

    let rows: Vec<&str> = screen.lines().collect();
    let strip = rows
        .iter()
        .position(|row| row.contains("Explorer - git"))
        .expect("the tab strip");
    assert!(
        rows[strip + 1].contains(&root_name),
        "the row under the strip is the root of the explorer: {screen}"
    );
    assert!(
        !rows[strip].contains(&root_name),
        "and not the strip's own row: {}",
        rows[strip]
    );
}

#[test]
fn a_plugin_view_starts_under_the_strip_too() {
    let mut app = app();
    let (x, y) = tab_of(&app, &ComponentKind::Explorer, 1);
    click_at(&mut app, MouseEventKind::Down(MouseButton::Left), x, y);
    app.state_mut().set_plugin_view(
        PluginViewId::new("git"),
        crate::ui::view::ViewNode::Lines(vec![crate::ui::view::ViewLine {
            text: "first row of the plugin".into(),
            slot: "".into(),
        }]),
    );

    let screen = draw(&mut app);

    let rows: Vec<&str> = screen.lines().collect();
    let strip = rows
        .iter()
        .position(|row| row.contains("Explorer - git"))
        .expect("the tab strip");
    assert!(
        rows[strip + 1].contains("first row of the plugin"),
        "{screen}"
    );
}
