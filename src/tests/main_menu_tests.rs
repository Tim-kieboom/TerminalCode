//! The main menu: the icon in the top row, `f10`, the groups and their items.

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::app::state::{AppState, Focus};
use crate::components::popups::menu::Entry;
use crate::components::{ComponentKind, prepare_and_render};
use crate::event::action::Action;

fn app() -> App {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    let mut state = AppState::default();
    state.open_project(dir.path()).unwrap();
    std::mem::forget(dir);
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

fn key(app: &mut App, code: KeyCode) {
    app.handle_input(InputEvent::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    draw(app);
}

fn click(app: &mut App, column: u16, row: u16) {
    app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
    draw(app);
}

fn labels(app: &App) -> Vec<String> {
    app.state()
        .menu()
        .unwrap()
        .entries()
        .iter()
        .map(|entry| match entry {
            Entry::Item { label, .. } | Entry::Group { label, .. } => label.to_string(),
            Entry::Separator => "---".to_owned(),
        })
        .collect()
}

#[test]
fn the_top_row_shows_the_icon_over_the_rest_of_the_screen() {
    let mut app = app();

    let screen = draw(&mut app);

    let first = screen.lines().next().unwrap();
    assert!(first.contains('≡'), "{first}");
    let placements = app
        .state()
        .placements(ratatui::layout::Rect::new(0, 0, 100, 30));
    let bar = placements
        .iter()
        .find(|p| p.kind == ComponentKind::MenuBar)
        .unwrap();
    assert_eq!((bar.area.y, bar.area.height), (0, 1));
}

#[test]
fn a_click_on_the_icon_opens_the_menu_under_it_and_leaves_the_keyboard_alone() {
    let mut app = app();
    app.state_mut().set_focus(Focus::Explorer);

    click(&mut app, 1, 0);

    assert_eq!(labels(&app), ["File", "View", "Settings", "---", "Quit"]);
    assert_eq!(app.state().focus(), Focus::Explorer);
    let screen = draw(&mut app);
    assert!(screen.contains("File"), "{screen}");
}

#[test]
fn f10_and_the_palette_entry_open_it_too() {
    let mut app = app();

    key(&mut app, KeyCode::F(10));
    assert!(app.state().menu().is_some());
    key(&mut app, KeyCode::Esc);
    assert!(app.state().menu().is_none());

    app.do_action(Action::OpenMenu);
    assert!(app.state().menu().is_some());
}

#[test]
fn escape_closes_the_menu_and_the_keyboard_is_where_it_was() {
    let mut app = app();
    app.state_mut().set_focus(Focus::Explorer);
    key(&mut app, KeyCode::F(10));

    key(&mut app, KeyCode::Esc);

    assert!(app.state().menu().is_none());
    assert_eq!(app.state().focus(), Focus::Explorer);
}

#[test]
fn a_click_outside_closes_it() {
    let mut app = app();
    key(&mut app, KeyCode::F(10));

    click(&mut app, 80, 20);

    assert!(app.state().menu().is_none());
}

#[test]
fn right_opens_a_group_and_enter_runs_its_item() {
    let mut app = app();
    app.state_mut().set_focus(Focus::Explorer);
    key(&mut app, KeyCode::F(10));
    // View is the second group; its first item is Toggle Explorer.
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Right);
    assert!(app.state().menu().unwrap().submenu().is_some());
    assert!(app.state().is_showing(&ComponentKind::Explorer));

    key(&mut app, KeyCode::Enter);

    assert!(app.state().menu().is_none());
    assert!(!app.state().is_showing(&ComponentKind::Explorer));
}

#[test]
fn a_click_on_a_group_and_then_on_its_item_runs_it() {
    let mut app = app();
    key(&mut app, KeyCode::F(10));
    // The menu hangs from row 0, so its first item (File) is on row 2.
    click(&mut app, 5, 2);
    assert!(app.state().menu().unwrap().submenu().is_some());
    let sub_x = {
        let menu = app.state().menu().unwrap();
        (0..100)
            .find(|x| {
                menu.submenu()
                    .unwrap()
                    .contains(ratatui::layout::Position::new(*x, 2))
            })
            .unwrap()
    };

    // New File is the first item of File.
    click(&mut app, sub_x + 2, 2);

    assert!(app.state().menu().is_none());
    assert!(app.state().workspace().has_tabs());
}

#[test]
fn every_item_shows_the_key_the_palette_shows() {
    let mut app = app();
    key(&mut app, KeyCode::F(10));
    key(&mut app, KeyCode::Right);

    let menu = app.state().menu().unwrap();
    let file = menu.submenu().unwrap();
    let save = file
        .entries()
        .iter()
        .find_map(|entry| match entry {
            Entry::Item { label, keys, .. } if label == "Save" => Some(keys.clone()),
            _ => None,
        })
        .unwrap();

    assert_eq!(save.as_deref(), Some("ctrl+s"));
}

#[test]
fn the_toggle_hides_the_row_and_the_menu_then_opens_at_the_top_left() {
    let mut app = app();

    app.do_action(Action::ToggleMenuBar);
    let screen = draw(&mut app);
    assert!(!screen.lines().next().unwrap().contains('≡'));

    key(&mut app, KeyCode::F(10));
    let screen = draw(&mut app);
    assert!(app.state().menu().is_some());
    let second = screen.lines().nth(1).unwrap();
    assert!(second.contains("File"), "{second}");
}

#[test]
fn a_layout_without_the_menu_bar_loads_and_has_no_row() {
    let mut app = app();
    app.state_mut().set_layout(
        crate::ui::layout::LayoutTree::from_ron(
            r#"Col([Pane(view: Editor), Fixed(1, Pane(view: StatusBar))])"#,
        )
        .unwrap(),
    );

    let screen = draw(&mut app);

    assert!(!screen.lines().next().unwrap().contains('≡'));
    // The menu still opens from the key.
    key(&mut app, KeyCode::F(10));
    assert!(app.state().menu().is_some());
}

#[test]
fn the_menu_bar_cannot_be_a_view_of_a_bar() {
    let result = crate::ui::layout::LayoutTree::from_ron(
        r#"Row([Fixed(20, SideBar(views: [MenuBar])), Pane(view: Editor)])"#,
    );

    assert!(result.is_err());
}

#[test]
fn the_state_remembers_a_hidden_menu_bar() {
    let mut state = crate::saved_state::SavedState::default();

    assert_eq!(state.shown(&ComponentKind::MenuBar), None);
    state.set_shown(&ComponentKind::MenuBar, false);

    assert_eq!(state.shown(&ComponentKind::MenuBar), Some(false));
}

#[test]
fn toggling_the_menu_bar_of_a_layout_that_has_none_says_so_and_remembers_nothing() {
    let mut app = app();
    app.state_mut().set_layout(
        crate::ui::layout::LayoutTree::from_ron(
            r#"Col([Pane(view: Editor), Fixed(1, Pane(view: StatusBar))])"#,
        )
        .unwrap(),
    );

    app.do_action(Action::ToggleMenuBar);

    assert!(
        app.state().is_visible(&ComponentKind::MenuBar),
        "the flag is as it was"
    );
    let told = app.state().notifications().texts().join("|");
    assert!(told.contains("no menu bar"), "{told}");
}
