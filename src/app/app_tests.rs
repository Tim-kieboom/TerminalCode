use super::*;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};

fn press(app: &mut App, code: KeyCode, modifiers: event::KeyModifiers) {
    let event = Event::Key(KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    });
    app.handle_event(event).unwrap();
}

#[test]
fn debug_window_scrolls_on_arrow_key() {
    let mut app = App::new(StartupArgs::new(".".into())).unwrap();
    app.log_note("one".to_string());
    app.log_note("two".to_string());
    app.log_note("three".to_string());

    press(
        &mut app,
        KeyCode::Char('d'),
        event::KeyModifiers::CONTROL | event::KeyModifiers::ALT | event::KeyModifiers::SHIFT,
    );
    press(&mut app, KeyCode::Down, event::KeyModifiers::NONE);
    press(&mut app, KeyCode::Down, event::KeyModifiers::NONE);

    assert_eq!(app.debug_window.inner().cursor_vertical(), 2);
}

#[test]
fn sidebar_tab_switches_on_prev_next_tab() {
    use super::components::sidebar::SideBarSelect;

    let mut app = App::new(StartupArgs::new(".".into())).unwrap();
    press(&mut app, KeyCode::Char('b'), event::KeyModifiers::ALT);

    press(&mut app, KeyCode::Char('='), event::KeyModifiers::ALT);
    assert_eq!(app.sidebar.inner().select, SideBarSelect::Debugger);

    press(&mut app, KeyCode::Char('-'), event::KeyModifiers::ALT);
    assert_eq!(app.sidebar.inner().select, SideBarSelect::Explorer);
}
