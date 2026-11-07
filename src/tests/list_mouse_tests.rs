//! The mouse over the plugin list: the pointer or a click selects a plugin, the wheel steps, and
//! a click outside closes the list unless something is being downloaded.

use crossterm::event::{
    Event as InputEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::app::state::{AppState, Popup};
use crate::components::popups::plugin_list::{Mark, PluginList, Row};
use crate::components::prepare_and_render;

fn row(id: &str) -> Row {
    Row {
        id: id.into(),
        name: format!("Plugin {id}"),
        version: "1.0.0".to_owned(),
        state: "not installed".to_owned(),
        source: None,
        detail: format!("what {id} does"),
        mark: Mark::NotInstalled,
        grey: false,
    }
}

fn app_with_list(count: usize, busy: bool) -> App {
    let mut app = App::new(AppState::default());
    let rows = (0..count).map(|n| row(&format!("p{n}"))).collect();
    let mut list = PluginList::new(rows, None);
    if busy {
        list.set_busy(Some("installing p0".to_owned()));
    }
    app.state_mut().open_plugin_list(list);
    app
}

fn screen(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    terminal.backend().to_string()
}

fn cell_of(app: &mut App, text: &str) -> (u16, u16) {
    let shown = screen(app);
    for (row, line) in shown.lines().enumerate() {
        let line = line.trim_matches('"');
        if let Some(byte) = line.find(text) {
            return (line[..byte].chars().count() as u16, row as u16);
        }
    }
    panic!("{text} is not on the screen: {shown}");
}

fn mouse(app: &mut App, kind: MouseEventKind, (column, row): (u16, u16)) {
    app.handle_input(InputEvent::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

fn chosen(app: &mut App) -> String {
    app.state_mut()
        .plugin_list_mut()
        .and_then(|list| list.selected_id().map(str::to_owned))
        .unwrap_or_default()
}

#[test]
fn the_pointer_over_a_plugin_selects_it() {
    let mut app = app_with_list(5, false);
    assert_eq!(chosen(&mut app), "p0");
    let at = cell_of(&mut app, "Plugin p3");

    mouse(&mut app, MouseEventKind::Moved, at);

    assert_eq!(chosen(&mut app), "p3");
}

#[test]
fn a_click_on_a_plugin_selects_it_and_does_nothing_else() {
    let mut app = app_with_list(5, false);
    let at = cell_of(&mut app, "Plugin p2");

    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), at);

    assert_eq!(chosen(&mut app), "p2");
    assert!(
        matches!(app.state().popup(), Popup::PluginList(_)),
        "still open"
    );
}

#[test]
fn the_wheel_steps_through_the_plugins() {
    let mut app = app_with_list(5, false);
    let at = cell_of(&mut app, "Plugin p0");

    mouse(&mut app, MouseEventKind::ScrollDown, at);
    mouse(&mut app, MouseEventKind::ScrollDown, at);
    assert_eq!(chosen(&mut app), "p2");
    mouse(&mut app, MouseEventKind::ScrollUp, at);
    assert_eq!(chosen(&mut app), "p1");
}

#[test]
fn a_click_outside_the_list_closes_it() {
    let mut app = app_with_list(3, false);
    screen(&mut app);

    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), (1, 1));

    assert!(!matches!(app.state().popup(), Popup::PluginList(_)));
}

#[test]
fn a_click_outside_while_something_downloads_keeps_the_list_open() {
    let mut app = app_with_list(3, true);
    screen(&mut app);

    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), (1, 1));

    assert!(matches!(app.state().popup(), Popup::PluginList(_)));
}

#[test]
fn a_click_on_the_boxes_text_under_the_rows_does_nothing() {
    let mut app = app_with_list(3, false);
    let (column, row) = cell_of(&mut app, "Plugin p1");

    // A line far below the rows is inside the box (the help lines), not on a plugin.
    mouse(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        (column, row + 9),
    );

    assert!(matches!(app.state().popup(), Popup::PluginList(_)));
    assert_eq!(chosen(&mut app), "p0");
}
