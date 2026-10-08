use std::io;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::component::PluginViewId;
use crate::error::IdeError;
use crate::event::Event;
use crate::state::AppState;
use crate::ui::view::{ViewLine, ViewNode};

use super::super::app::*;

fn key(code: KeyCode, modifiers: KeyModifiers) -> InputEvent {
    InputEvent::Key(KeyEvent::new(code, modifiers))
}

fn ctrl_q() -> InputEvent {
    key(KeyCode::Char('q'), KeyModifiers::CONTROL)
}

fn plugin_text(text: &str) -> Event {
    Event::SetPluginView {
        id: PluginViewId::new("test.view"),
        content: ViewNode::Lines(vec![ViewLine {
            text: text.into(),
            slot: "none".into(),
        }]),
    }
}

fn test_sources() -> (Sources, mpsc::Sender<InputResult>, mpsc::Sender<Event>) {
    let (input_tx, input) = mpsc::channel(8);
    let (events_tx, events) = mpsc::channel(8);
    (Sources::new(input, events), input_tx, events_tx)
}

#[test]
fn new_app_wants_a_first_draw() {
    assert!(App::default().needs_redraw());
}

#[test]
fn ctrl_q_quits() {
    let mut app = App::default();

    app.handle_input(ctrl_q());

    assert!(app.should_quit());
}

#[test]
fn plain_q_does_not_quit() {
    let mut app = App::default();

    app.handle_input(key(KeyCode::Char('q'), KeyModifiers::NONE));

    assert!(!app.should_quit());
}

#[test]
fn key_release_does_not_quit() {
    let mut app = App::default();
    let mut release = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
    release.kind = KeyEventKind::Release;

    app.handle_input(InputEvent::Key(release));

    assert!(!app.should_quit());
}

#[test]
fn resize_requests_a_redraw() {
    let mut app = App::default();
    app.mark_drawn();

    app.handle_input(InputEvent::Resize(80, 24));

    assert!(app.needs_redraw());
}

#[test]
fn plugin_view_event_updates_state_and_requests_redraw() {
    let mut app = App::default();
    app.mark_drawn();

    app.handle_event(plugin_text("hello"));

    let view = app.state().plugin_view(&PluginViewId::new("test.view"));
    assert!(view.is_some());
    assert!(app.needs_redraw());
}

#[tokio::test]
async fn loop_draws_first_frame_then_quits_on_ctrl_q() {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let (sources, input_tx, _events_tx) = test_sources();
    input_tx.send(Ok(ctrl_q())).await.unwrap();

    let result = run(&mut terminal, sources, AppState::default()).await;

    assert!(result.is_ok());
    let screen = terminal.backend().to_string();
    assert!(
        screen.contains("Explorer"),
        "first frame was not drawn:\n{screen}"
    );
}

#[tokio::test]
async fn loop_accepts_events_before_quit() {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let (sources, input_tx, events_tx) = test_sources();
    events_tx.send(plugin_text("hi")).await.unwrap();
    input_tx.send(Ok(ctrl_q())).await.unwrap();

    let result = run(&mut terminal, sources, AppState::default()).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn loop_returns_input_errors() {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let (sources, input_tx, _events_tx) = test_sources();
    let err = io::Error::other("boom");
    input_tx.send(Err(err)).await.unwrap();

    let result = run(&mut terminal, sources, AppState::default()).await;

    assert!(matches!(result, Err(IdeError::Io(_))));
}

#[tokio::test]
async fn loop_ends_when_input_closes() {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let (sources, input_tx, _events_tx) = test_sources();
    drop(input_tx);

    let result = run(&mut terminal, sources, AppState::default()).await;

    assert!(result.is_ok());
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_input(key(code, modifiers));
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c), KeyModifiers::NONE);
    }
}

fn buffer_text(app: &App) -> String {
    app.state().editor().buffer().text()
}

#[test]
fn typing_inserts_text_into_the_editor() {
    let mut app = App::default();

    type_str(&mut app, "hi!");

    assert_eq!(buffer_text(&app), "hi!");
}

#[test]
fn shifted_letters_are_typed_as_uppercase() {
    let mut app = App::default();

    press(&mut app, KeyCode::Char('A'), KeyModifiers::SHIFT);

    assert_eq!(buffer_text(&app), "A");
}

#[test]
fn enter_backspace_and_tab_edit_the_text() {
    let mut app = App::default();

    type_str(&mut app, "ab");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);

    assert_eq!(buffer_text(&app), "ab\n   ");
}

#[test]
fn arrows_move_the_cursor_and_shift_arrows_select() {
    let mut app = App::default();
    type_str(&mut app, "abc");

    press(&mut app, KeyCode::Left, KeyModifiers::NONE);
    press(&mut app, KeyCode::Left, KeyModifiers::SHIFT);
    press(&mut app, KeyCode::Delete, KeyModifiers::NONE);

    assert_eq!(buffer_text(&app), "ac");
}

#[test]
fn ctrl_z_and_ctrl_y_undo_and_redo() {
    let mut app = App::default();
    type_str(&mut app, "ab");

    press(&mut app, KeyCode::Char('z'), KeyModifiers::CONTROL);
    assert_eq!(buffer_text(&app), "a");

    press(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL);
    assert_eq!(buffer_text(&app), "ab");
}

#[test]
fn unbound_modified_keys_do_nothing() {
    let mut app = App::default();
    app.mark_drawn();

    press(&mut app, KeyCode::Char('x'), KeyModifiers::CONTROL);

    assert_eq!(buffer_text(&app), "");
    assert!(!app.needs_redraw());
}

#[test]
fn handled_keys_request_a_redraw() {
    let mut app = App::default();
    app.mark_drawn();

    type_str(&mut app, "a");

    assert!(app.needs_redraw());
}

#[test]
fn ctrl_s_saves_and_reports_it_in_the_status() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.txt");
    let editor = crate::editor::Editor::new(crate::buffer::Buffer::open_or_new(&path).unwrap());
    let mut app = App::new(AppState::new(editor));
    type_str(&mut app, "saved text");

    press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "saved text");
    assert_eq!(app.state().status(), Some("saved note.txt"));
}

#[test]
fn saving_a_buffer_without_a_path_reports_an_error_and_keeps_running() {
    let mut app = App::default();
    type_str(&mut app, "x");

    press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);

    let status = app.state().status().unwrap();
    assert!(status.contains("no file path"), "status was {status:?}");
    assert!(!app.should_quit());
}

#[test]
fn the_next_handled_key_clears_the_status() {
    let mut app = App::default();
    press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
    assert!(app.state().status().is_some());

    type_str(&mut app, "a");

    assert_eq!(app.state().status(), None);
}
