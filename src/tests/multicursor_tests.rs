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

// ---- copy, cut and paste

fn clipboard_app(text: &str) -> App {
    let memory = crate::clipboard::Memory::default();
    let clipboard = crate::clipboard::Clipboard::new(crate::clipboard::System::Memory(memory));
    app(text).with_clipboard(clipboard)
}

fn ctrl(app: &mut App, c: char) {
    press(app, KeyCode::Char(c), KeyModifiers::CONTROL);
}

/// Cursors on the first `lines` lines.
fn cursors_down(app: &mut App, lines: usize) {
    for _ in 1..lines {
        press(app, KeyCode::Down, ALT_CTRL);
    }
}

fn select_two_letters(app: &mut App) {
    press(app, KeyCode::Right, KeyModifiers::SHIFT);
    press(app, KeyCode::Right, KeyModifiers::SHIFT);
}

#[test]
fn copy_joins_the_selections_and_one_cursor_pastes_all_of_it() {
    let mut app = clipboard_app("ab1\ncd2\nef3\n\n");
    cursors_down(&mut app, 3);
    select_two_letters(&mut app);
    ctrl(&mut app, 'c');
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.state_mut()
        .edit(|editor| editor.go_to(Position::new(3, 0)));

    ctrl(&mut app, 'v');

    assert_eq!(text(&app), "ab1\ncd2\nef3\nab\ncd\nef\n");
}

#[test]
fn a_one_line_copy_is_pasted_at_every_cursor() {
    let mut app = clipboard_app("pub \nmod app;\nmod buffer;\n");
    press(&mut app, KeyCode::Right, KeyModifiers::SHIFT);
    for _ in 0..3 {
        press(&mut app, KeyCode::Right, KeyModifiers::SHIFT);
    }
    ctrl(&mut app, 'c');
    app.state_mut()
        .edit(|editor| editor.go_to(Position::new(1, 0)));
    press(&mut app, KeyCode::Down, ALT_CTRL);

    ctrl(&mut app, 'v');

    assert_eq!(text(&app), "pub \npub mod app;\npub mod buffer;\n");
    assert_eq!(cursors(&app), 2);
}

#[test]
fn as_many_lines_as_cursors_go_one_to_each_cursor() {
    let mut app = clipboard_app("ab1\ncd2\nef3\n\nx\ny\nz");
    cursors_down(&mut app, 3);
    select_two_letters(&mut app);
    ctrl(&mut app, 'c');
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.state_mut()
        .edit(|editor| editor.go_to(Position::new(4, 0)));
    cursors_down(&mut app, 3);

    ctrl(&mut app, 'v');

    assert_eq!(text(&app), "ab1\ncd2\nef3\n\nabx\ncdy\nefz");
}

#[test]
fn another_number_of_lines_goes_whole_to_every_cursor() {
    let mut app = clipboard_app("ab1\ncd2\nef3");
    cursors_down(&mut app, 3);
    select_two_letters(&mut app);
    ctrl(&mut app, 'c');
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    app.state_mut()
        .edit(|editor| editor.go_to(Position::new(0, 0)));
    press(&mut app, KeyCode::Down, ALT_CTRL);

    ctrl(&mut app, 'v');

    assert_eq!(text(&app).matches("ab\ncd\nef").count(), 2);
    assert_eq!(cursors(&app), 2);
}

#[test]
fn cut_takes_the_selections_out_in_one_undo_step() {
    let mut app = clipboard_app("ab1\ncd2\nef3");
    cursors_down(&mut app, 3);
    select_two_letters(&mut app);

    ctrl(&mut app, 'x');
    assert_eq!(text(&app), "1\n2\n3");
    assert_eq!(cursors(&app), 3);

    ctrl(&mut app, 'z');
    assert_eq!(text(&app), "ab1\ncd2\nef3");
}

#[test]
fn bare_cursors_copy_their_lines_and_paste_one_above_each_line() {
    let mut app = clipboard_app("a\nb\nc");
    cursors_down(&mut app, 2);
    ctrl(&mut app, 'c');

    ctrl(&mut app, 'v');

    assert_eq!(text(&app), "a\na\nb\nb\nc");
    assert_eq!(cursors(&app), 2);
}

#[test]
fn bare_cursors_on_one_line_copy_it_once_and_paste_it_once() {
    let mut app = clipboard_app("abcd\nz");
    app.state_mut().edit(|editor| {
        editor.add_cursor(Position::new(0, 2));
    });
    assert_eq!(cursors(&app), 2);
    ctrl(&mut app, 'c');

    ctrl(&mut app, 'v');

    assert_eq!(text(&app), "abcd\nabcd\nz");
}

#[test]
fn cut_with_bare_cursors_removes_their_lines() {
    let mut app = clipboard_app("a\nb\nc\nd");
    cursors_down(&mut app, 2);

    ctrl(&mut app, 'x');
    assert_eq!(text(&app), "c\nd");

    ctrl(&mut app, 'z');
    assert_eq!(text(&app), "a\nb\nc\nd");
}

#[test]
fn a_copy_with_a_selection_and_a_bare_cursor_pastes_as_plain_text() {
    let mut app = clipboard_app("ab\ncd\n\n");
    press(&mut app, KeyCode::Right, KeyModifiers::SHIFT);
    press(&mut app, KeyCode::Right, KeyModifiers::SHIFT);
    app.state_mut()
        .edit(|editor| editor.add_cursor(Position::new(1, 0)));
    ctrl(&mut app, 'c');
    app.state_mut().edit(|editor| editor.collapse_cursors());
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);

    ctrl(&mut app, 'v');

    assert!(text(&app).ends_with("ab\ncd"), "{}", text(&app));
}

// ---- comments

#[test]
fn ctrl_slash_comments_the_line_and_a_file_with_no_language_says_so() {
    let mut buffer = Buffer::from_text("let x = 1;");
    buffer.set_path(std::path::PathBuf::from("a.rs"));
    let mut rust = App::new(AppState::new(Editor::new(buffer)));
    press(&mut rust, KeyCode::Char('/'), KeyModifiers::CONTROL);
    assert_eq!(text(&rust), "// let x = 1;");

    let mut plain = app("hello");
    press(&mut plain, KeyCode::Char('/'), KeyModifiers::CONTROL);
    assert_eq!(text(&plain), "hello");
    let notes: Vec<&str> = plain.state().notifications().texts();
    assert!(
        notes.iter().any(|note| note.contains("no comment marker")),
        "{notes:?}"
    );
}
