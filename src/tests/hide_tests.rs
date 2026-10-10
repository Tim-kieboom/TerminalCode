use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::app::state::{AppState, Focus};
use crate::components::{ComponentKind, PluginViewId, prepare_and_render};
use crate::event::Event;
use crate::keymap::Keymap;
use crate::terminal::KeyboardSupport;
use crate::ui::layout::LayoutTree;
use crate::ui::view::{ViewLine, ViewNode};

fn key(code: KeyCode, modifiers: KeyModifiers) -> InputEvent {
    InputEvent::Key(KeyEvent::new(code, modifiers))
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_input(key(code, KeyModifiers::NONE));
}

fn ctrl(app: &mut App, c: char) {
    app.handle_input(key(KeyCode::Char(c), KeyModifiers::CONTROL));
}

/// `ctrl+k` followed by `c`.
fn prefix(app: &mut App, c: char) {
    ctrl(app, 'k');
    press(app, KeyCode::Char(c));
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn screen(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    terminal.backend().to_string()
}

fn editor_area(app: &mut App) -> ratatui::layout::Rect {
    screen(app);
    app.state().workspace().focused_area()
}

#[test]
fn everything_starts_shown() {
    let state = AppState::default();

    assert!(state.is_visible(&ComponentKind::Explorer));
    assert!(state.is_visible(&ComponentKind::StatusBar));
    assert!(state.is_visible(&ComponentKind::Editor));
}

#[test]
fn ctrl_k_b_hides_the_explorer_and_the_editor_takes_its_space() {
    let mut app = App::default();
    let before = editor_area(&mut app);
    assert!(screen(&mut app).contains("Explorer"));

    prefix(&mut app, 'b');

    assert!(!app.state().is_visible(&ComponentKind::Explorer));
    let after = editor_area(&mut app);
    assert_eq!(after.width, before.width + 30);
    assert_eq!(after.x, 0);
    assert!(!screen(&mut app).contains("Explorer"));
}

#[test]
fn toggling_again_shows_it_with_the_keyboard() {
    let mut app = App::default();
    prefix(&mut app, 'b');

    prefix(&mut app, 'b');

    assert!(app.state().is_visible(&ComponentKind::Explorer));
    assert_eq!(app.state().focus(), Focus::Explorer);
    assert!(screen(&mut app).contains("Explorer"));
}

#[test]
fn hiding_the_explorer_while_it_has_the_keyboard_returns_it_to_the_editor() {
    let mut app = App::default();
    ctrl(&mut app, 'b');
    assert_eq!(app.state().focus(), Focus::Explorer);

    prefix(&mut app, 'b');

    assert_eq!(app.state().focus(), Focus::Editor);
    type_str(&mut app, "abc");
    assert_eq!(app.state().editor().buffer().text(), "abc");
}

#[test]
fn ctrl_b_shows_a_hidden_explorer_and_focuses_it() {
    let mut app = App::default();
    prefix(&mut app, 'b');

    ctrl(&mut app, 'b');

    assert!(app.state().is_visible(&ComponentKind::Explorer));
    assert_eq!(app.state().focus(), Focus::Explorer);
}

#[test]
fn a_click_where_the_hidden_explorer_was_goes_to_the_editor() {
    let mut app = App::default();
    screen(&mut app);
    prefix(&mut app, 'b');
    screen(&mut app);

    app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 3,
        row: 3,
        modifiers: KeyModifiers::NONE,
    }));

    assert_eq!(app.state().focus(), Focus::Editor);
}

#[test]
fn the_explorer_keeps_its_state_while_hidden() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("sub/inner.txt"), "").unwrap();
    let mut state = AppState::default();
    state.open_project(dir.path()).unwrap();
    let mut app = App::new(state);
    ctrl(&mut app, 'b');
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Right);
    let rows = app.state().explorer().rows().len();
    assert!(rows > 2);

    prefix(&mut app, 'b');
    prefix(&mut app, 'b');

    assert_eq!(app.state().explorer().rows().len(), rows);
    assert_eq!(app.state().explorer().selected(), 1);
}

#[test]
fn ctrl_k_s_hides_the_status_bar_and_gives_its_row_back() {
    let mut app = App::default();
    let before = editor_area(&mut app);
    assert!(screen(&mut app).contains("Ln 1, Col 1"));

    prefix(&mut app, 's');

    assert!(!app.state().is_visible(&ComponentKind::StatusBar));
    assert!(!screen(&mut app).contains("Ln 1, Col 1"));
    // The terminal placeholder is fixed height, so the editor column is the
    // one that gets the row.
    assert!(editor_area(&mut app).height > before.height);

    prefix(&mut app, 's');
    assert!(screen(&mut app).contains("Ln 1, Col 1"));
    assert_eq!(editor_area(&mut app), before);
}

#[test]
fn the_palette_lists_both_toggles_with_their_keys() {
    let mut app = App::default();

    press(&mut app, KeyCode::F(1));
    type_str(&mut app, "toggle");

    let entries: Vec<_> = app
        .state()
        .palette()
        .unwrap()
        .visible()
        .map(|entry| (entry.title, entry.keys.clone()))
        .collect();
    assert!(
        entries.contains(&("View: Toggle Explorer", Some("ctrl+k b".to_owned()))),
        "{entries:?}"
    );
    assert!(
        entries.contains(&("View: Toggle Status Bar", Some("ctrl+k s".to_owned()))),
        "{entries:?}"
    );
}

#[test]
fn the_toggles_work_from_the_palette_too() {
    let mut app = App::default();
    press(&mut app, KeyCode::F(1));
    type_str(&mut app, "toggle status");

    press(&mut app, KeyCode::Enter);

    assert!(!app.state().is_visible(&ComponentKind::StatusBar));
}

#[test]
fn a_layout_without_an_explorer_says_so() {
    let mut app = App::default();
    app.state_mut()
        .set_layout(LayoutTree::from_ron("Pane(view: Editor)").unwrap());

    prefix(&mut app, 'b');

    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("no explorer")
    );
}

const PLUGIN_LAYOUT: &str = r#"Row([
    Pane(view: Editor),
    Fixed(20, Pane(view: Plugin("test.view"))),
])"#;

fn plugin_text(text: &str) -> Event {
    Event::SetPluginView {
        id: PluginViewId::new("test.view"),
        content: ViewNode::Lines(vec![ViewLine {
            text: text.into(),
            slot: "none".into(),
        }]),
    }
}

fn app_with_plugin() -> App {
    let mut keymap = Keymap::defaults(KeyboardSupport::Enhanced);
    keymap
        .apply_toml(
            r#"[[binding]]
keys = "ctrl+t"
action = { toggle_plugin_view = "test.view" }
[[binding]]
keys = "f9"
action = { toggle_plugin_view = "no.such.view" }"#,
        )
        .unwrap();
    let mut state = AppState::default();
    state.set_layout(LayoutTree::from_ron(PLUGIN_LAYOUT).unwrap());
    let mut app = App::with_keymap(state, keymap);
    app.handle_event(plugin_text("plugin says hi"));
    app
}

#[test]
fn a_plugin_view_can_be_hidden_and_shown_by_its_id() {
    let mut app = app_with_plugin();
    let before = editor_area(&mut app);
    assert!(screen(&mut app).contains("plugin says hi"));

    ctrl(&mut app, 't');

    assert!(!screen(&mut app).contains("plugin says hi"));
    assert_eq!(editor_area(&mut app).width, before.width + 20);

    ctrl(&mut app, 't');
    assert!(screen(&mut app).contains("plugin says hi"));
}

#[test]
fn a_plugin_update_does_not_show_a_view_the_user_hid() {
    let mut app = app_with_plugin();
    ctrl(&mut app, 't');

    app.handle_event(plugin_text("a newer text"));

    assert!(!screen(&mut app).contains("a newer text"));
    let version = app
        .state()
        .plugin_view(&PluginViewId::new("test.view"))
        .unwrap()
        .version();
    assert_eq!(version, 1, "the update was still taken");

    ctrl(&mut app, 't');
    assert!(screen(&mut app).contains("a newer text"));
}

#[test]
fn toggling_a_plugin_view_that_does_not_exist_says_so() {
    let mut app = app_with_plugin();

    press(&mut app, KeyCode::F(9));

    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("no.such.view")
    );
}

#[test]
fn a_view_with_no_plugin_content_yet_still_has_its_pane() {
    let mut state = AppState::default();
    state.set_layout(LayoutTree::from_ron(PLUGIN_LAYOUT).unwrap());
    let mut app = App::new(state);

    let editor = editor_area(&mut app);

    assert_eq!(
        editor.width, 80,
        "the pane for the plugin is there: {editor:?}"
    );
}
