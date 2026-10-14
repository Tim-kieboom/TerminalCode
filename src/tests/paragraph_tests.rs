//! `ctrl+up/down` and `ctrl+shift+up/down` reach the paragraph motion through
//! the default keymap.

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::app::state::AppState;
use crate::buffer::{Buffer, Position};
use crate::components::editor::Editor;

fn app() -> App {
    let buffer = Buffer::from_text("one\ntwo\n\nthree\nfour");
    App::new(AppState::new(Editor::new(buffer)))
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
}

fn primary(app: &App) -> (Position, Position) {
    let selection = app.state().editor().selections().primary();
    (selection.anchor(), selection.head())
}

#[test]
fn ctrl_down_and_ctrl_up_move_by_paragraph() {
    let mut app = app();

    press(&mut app, KeyCode::Down, KeyModifiers::CONTROL);
    assert_eq!(primary(&app), (Position::new(2, 0), Position::new(2, 0)));

    press(&mut app, KeyCode::Down, KeyModifiers::CONTROL);
    assert_eq!(primary(&app), (Position::new(4, 4), Position::new(4, 4)));

    press(&mut app, KeyCode::Up, KeyModifiers::CONTROL);
    assert_eq!(primary(&app), (Position::new(2, 0), Position::new(2, 0)));
}

#[test]
fn ctrl_shift_down_selects_up_to_the_blank_line() {
    let mut app = app();

    press(
        &mut app,
        KeyCode::Down,
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );

    assert_eq!(primary(&app), (Position::new(0, 0), Position::new(2, 0)));
}
