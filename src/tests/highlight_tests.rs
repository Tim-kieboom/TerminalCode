use std::fs;
use std::path::Path;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;

use crate::app::App;
use crate::components::prepare_and_render;
use crate::initial_state;

/// The default theme's keyword color.
const KEYWORD: Color = Color::Rgb(0xcb, 0xa6, 0xf7);

fn app_with(dir: &Path, name: &str, text: &str) -> App {
    let path = dir.join(name);
    fs::write(&path, text).unwrap();
    App::new(initial_state(&[path]).unwrap())
}

fn draw(app: &mut App) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    terminal
}

fn key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    app.handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
}

/// The color of the first character of the first `text` found on screen.
fn color_of(terminal: &Terminal<TestBackend>, text: &str) -> Option<Color> {
    let buffer = terminal.backend().buffer();
    let width = usize::from(buffer.area.width);
    let cells = &buffer.content;
    for row in 0..usize::from(buffer.area.height) {
        let line: String = (0..width)
            .map(|col| cells[row * width + col].symbol())
            .collect();
        if let Some(col) = line.find(text) {
            let col = line[..col].chars().count();
            return cells[row * width + col].fg.into();
        }
    }
    panic!("{text} is not on screen");
}

#[test]
fn rust_keywords_are_drawn_in_the_theme_color() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with(dir.path(), "a.rs", "fn main() {}\n");

    let terminal = draw(&mut app);

    assert_eq!(color_of(&terminal, "fn main"), Some(KEYWORD));
}

#[test]
fn text_in_a_file_of_no_known_language_stays_plain() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with(dir.path(), "a.txt", "fn main() {}\n");

    let terminal = draw(&mut app);

    assert_ne!(color_of(&terminal, "fn main"), Some(KEYWORD));
}

#[test]
fn typing_recolors_the_new_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with(dir.path(), "a.rs", "\n");
    draw(&mut app);

    for c in "fn".chars() {
        key(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
    }
    let terminal = draw(&mut app);

    assert_eq!(color_of(&terminal, "fn"), Some(KEYWORD));
}

#[test]
fn a_selection_keeps_the_syntax_color_of_its_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = app_with(dir.path(), "a.rs", "fn main() {}\n");
    draw(&mut app);

    key(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    let terminal = draw(&mut app);

    assert_eq!(color_of(&terminal, "fn main"), Some(KEYWORD));
}

#[test]
fn text_scrolled_into_view_is_colored_too() {
    let dir = tempfile::tempdir().unwrap();
    let text: String = (0..300).map(|n| format!("fn f{n}() {{}}\n")).collect();
    let mut app = app_with(dir.path(), "a.rs", &text);
    draw(&mut app);

    key(&mut app, KeyCode::End, KeyModifiers::CONTROL);
    let terminal = draw(&mut app);

    assert_eq!(color_of(&terminal, "fn f299"), Some(KEYWORD));
}

#[test]
fn a_second_pane_scrolled_elsewhere_is_colored_as_well() {
    let dir = tempfile::tempdir().unwrap();
    let text: String = (0..300).map(|n| format!("fn f{n}() {{}}\n")).collect();
    let mut app = app_with(dir.path(), "a.rs", &text);
    draw(&mut app);
    // Split, then send the focused (new) pane to the end of the file.
    key(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    key(&mut app, KeyCode::Right, KeyModifiers::NONE);
    key(&mut app, KeyCode::End, KeyModifiers::CONTROL);

    let terminal = draw(&mut app);

    assert_eq!(color_of(&terminal, "fn f0()"), Some(KEYWORD), "left pane");
    assert_eq!(color_of(&terminal, "fn f299"), Some(KEYWORD), "right pane");
}
