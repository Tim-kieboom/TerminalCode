use ratatui::backend::TestBackend;

use crate::component::PluginViewId;
use crate::ui::view::{ViewLine, ViewNode};

use super::*;

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
    (Sources { input, events }, input_tx, events_tx)
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

    let result = run(&mut terminal, sources).await;

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

    let result = run(&mut terminal, sources).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn loop_returns_input_errors() {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let (sources, input_tx, _events_tx) = test_sources();
    let err = io::Error::other("boom");
    input_tx.send(Err(err)).await.unwrap();

    let result = run(&mut terminal, sources).await;

    assert!(matches!(result, Err(IdeError::Io(_))));
}

#[tokio::test]
async fn loop_ends_when_input_closes() {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let (sources, input_tx, _events_tx) = test_sources();
    drop(input_tx);

    let result = run(&mut terminal, sources).await;

    assert!(result.is_ok());
}
