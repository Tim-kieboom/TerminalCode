//! Several cursors through the keymap: adding one, typing at all of them,
//! going back to one, and how they are drawn.

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::app::state::AppState;
use crate::buffer::{Buffer, Position};
use crate::components::editor::Editor;
use crate::components::prepare_and_render;

const ALT_CTRL: KeyModifiers = KeyModifiers::CONTROL.union(KeyModifiers::ALT);

fn app(text: &str) -> App {
    App::new(AppState::new(Editor::new(Buffer::from_text(text))))
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c), KeyModifiers::NONE);
    }
}

fn text(app: &App) -> String {
    app.state().editor().buffer().text()
}

fn cursors(app: &App) -> usize {
    app.state().editor().selections().len()
}

#[test]
fn ctrl_alt_down_adds_a_cursor_and_typing_goes_to_all_of_them() {
    let mut app = app("a\nb\nc");

    press(&mut app, KeyCode::Down, ALT_CTRL);
    press(&mut app, KeyCode::Down, ALT_CTRL);
    type_str(&mut app, "x");

    assert_eq!(cursors(&app), 3);
    assert_eq!(text(&app), "xa\nxb\nxc");
}

#[test]
fn ctrl_alt_up_adds_a_cursor_above() {
    let mut app = app("a\nb\nc");
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Up, ALT_CTRL);

    assert_eq!(cursors(&app), 2);
}

#[test]
fn escape_goes_back_to_the_primary_cursor() {
    let mut app = app("a\nb\nc");
    press(&mut app, KeyCode::Down, ALT_CTRL);
    press(&mut app, KeyCode::Down, ALT_CTRL);

    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!(cursors(&app), 1);
    assert_eq!(
        app.state().editor().selections().primary().head(),
        Position::new(2, 0)
    );
}

#[test]
fn one_undo_takes_back_what_was_typed_at_every_cursor() {
    let mut app = app("a\nb");
    press(&mut app, KeyCode::Down, ALT_CTRL);
    type_str(&mut app, "xy");
    assert_eq!(text(&app), "xya\nxyb");

    press(&mut app, KeyCode::Char('z'), KeyModifiers::CONTROL);

    assert_eq!(text(&app), "a\nb");
    assert_eq!(cursors(&app), 2);
}

#[test]
fn an_indent_acts_on_the_primary_cursor_alone_and_leaves_one_cursor() {
    let mut app = app("a\nb");
    press(&mut app, KeyCode::Down, ALT_CTRL);

    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);

    assert_eq!(cursors(&app), 1);
}

fn drawn(app: &mut App) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    terminal
}

#[test]
fn every_cursor_but_the_primary_is_drawn_as_a_block() {
    let mut app = app("abc\ndef\nghi");
    press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    press(&mut app, KeyCode::Down, ALT_CTRL);
    press(&mut app, KeyCode::Down, ALT_CTRL);

    let terminal = drawn(&mut app);

    let extra = app.state().theme().style("cursor.extra").bg.unwrap();
    let marked: Vec<(u16, u16)> = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .enumerate()
        .filter(|(_, cell)| cell.bg == extra)
        .map(|(at, _)| (at as u16 % 60, at as u16 / 60))
        .collect();
    // The two further cursors are marked; the primary one (line 3) is the
    // terminal's own cursor.
    assert_eq!(marked.len(), 2, "{marked:?}");
}

#[test]
fn a_selection_is_drawn_for_every_cursor() {
    let mut app = app("abc\ndef");
    press(&mut app, KeyCode::Down, ALT_CTRL);
    press(&mut app, KeyCode::Right, KeyModifiers::SHIFT);

    let terminal = drawn(&mut app);

    let selection = app.state().theme().style("editor.selection");
    let selected = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .filter(|cell| cell.bg == selection.bg.unwrap())
        .count();
    assert_eq!(selected, 2, "one cell on each of the two lines");
}

fn mouse_at(app: &mut App, kind: MouseEventKind, column: u16, row: u16, modifiers: KeyModifiers) {
    app.handle_input(InputEvent::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers,
    }));
}

#[test]
fn an_alt_click_adds_a_cursor_that_typing_reaches_and_a_plain_click_removes() {
    let mut app = app("abc\ndef");
    let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    let area = app.state().editor().text_area().unwrap();

    mouse_at(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        area.x + 1,
        area.y,
        KeyModifiers::NONE,
    );
    mouse_at(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        area.x + 2,
        area.y + 1,
        KeyModifiers::ALT,
    );
    type_str(&mut app, "-");

    assert_eq!(text(&app), "a-bc\nde-f");
    assert_eq!(cursors(&app), 2);

    mouse_at(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        area.x,
        area.y,
        KeyModifiers::NONE,
    );
    assert_eq!(cursors(&app), 1);
}

#[test]
fn alt_down_and_alt_up_move_the_line() {
    let mut app = app("a\nb\nc");

    press(&mut app, KeyCode::Down, KeyModifiers::ALT);
    assert_eq!(text(&app), "b\na\nc");

    press(&mut app, KeyCode::Up, KeyModifiers::ALT);
    assert_eq!(text(&app), "a\nb\nc");
}
