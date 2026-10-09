use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use tokio::sync::mpsc;

use crate::app::App;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::initial_state;

/// The default theme's keyword color.
const KEYWORD: Color = Color::Rgb(0xcb, 0xa6, 0xf7);

/// An app wired to an event channel, as in the real app, so that the syntax
/// worker has somewhere to send its answers.
struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
}

fn app_with(dir: &Path, name: &str, text: &str) -> Rig {
    let path = dir.join(name);
    fs::write(&path, text).unwrap();
    let (tx, events) = mpsc::channel(256);
    let app = App::new(initial_state(&[path]).unwrap()).with_events(tx);
    Rig { app, events }
}

/// Draws, then keeps taking the worker's answers and drawing until it owes
/// none, and returns the last frame.
fn draw(rig: &mut Rig) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        terminal
            .draw(|frame| prepare_and_render(frame, rig.app.state_mut()))
            .unwrap();
        if !rig.app.state().workspace().highlights_pending() {
            return terminal;
        }
        loop {
            assert!(
                Instant::now() < deadline,
                "the syntax worker never answered"
            );
            match rig.events.try_recv() {
                Ok(event @ Event::Highlighted(_)) => {
                    rig.app.handle_event(event);
                    break;
                }
                Ok(_) => {}
                Err(_) => std::thread::sleep(Duration::from_millis(2)),
            }
        }
    }
}

fn key(rig: &mut Rig, code: KeyCode, modifiers: KeyModifiers) {
    rig.app
        .handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
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
    let mut rig = app_with(dir.path(), "a.rs", "fn main() {}\n");

    let terminal = draw(&mut rig);

    assert_eq!(color_of(&terminal, "fn main"), Some(KEYWORD));
}

#[test]
fn text_in_a_file_of_no_known_language_stays_plain() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = app_with(dir.path(), "a.txt", "fn main() {}\n");

    let terminal = draw(&mut rig);

    assert_ne!(color_of(&terminal, "fn main"), Some(KEYWORD));
}

#[test]
fn typing_recolors_the_new_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = app_with(dir.path(), "a.rs", "\n");
    draw(&mut rig);

    for c in "fn".chars() {
        key(&mut rig, KeyCode::Char(c), KeyModifiers::NONE);
    }
    let terminal = draw(&mut rig);

    assert_eq!(color_of(&terminal, "fn"), Some(KEYWORD));
}

#[test]
fn a_selection_keeps_the_syntax_color_of_its_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = app_with(dir.path(), "a.rs", "fn main() {}\n");
    draw(&mut rig);

    key(&mut rig, KeyCode::Char('a'), KeyModifiers::CONTROL);
    let terminal = draw(&mut rig);

    assert_eq!(color_of(&terminal, "fn main"), Some(KEYWORD));
}

#[test]
fn text_scrolled_into_view_is_colored_too() {
    let dir = tempfile::tempdir().unwrap();
    let text: String = (0..300).map(|n| format!("fn f{n}() {{}}\n")).collect();
    let mut rig = app_with(dir.path(), "a.rs", &text);
    draw(&mut rig);

    key(&mut rig, KeyCode::End, KeyModifiers::CONTROL);
    let terminal = draw(&mut rig);

    assert_eq!(color_of(&terminal, "fn f299"), Some(KEYWORD));
}

#[test]
fn a_second_pane_scrolled_elsewhere_is_colored_as_well() {
    let dir = tempfile::tempdir().unwrap();
    let text: String = (0..300).map(|n| format!("fn f{n}() {{}}\n")).collect();
    let mut rig = app_with(dir.path(), "a.rs", &text);
    draw(&mut rig);
    // Split, then send the focused (new) pane to the end of the file.
    key(&mut rig, KeyCode::Char('k'), KeyModifiers::CONTROL);
    key(&mut rig, KeyCode::Right, KeyModifiers::NONE);
    key(&mut rig, KeyCode::End, KeyModifiers::CONTROL);

    let terminal = draw(&mut rig);

    assert_eq!(color_of(&terminal, "fn f0()"), Some(KEYWORD), "left pane");
    assert_eq!(color_of(&terminal, "fn f299"), Some(KEYWORD), "right pane");
}

#[test]
fn typing_reparses_from_the_previous_tree_instead_of_from_scratch() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = app_with(dir.path(), "a.rs", "fn main() {}\n");
    draw(&mut rig);

    for c in "abc".chars() {
        key(&mut rig, KeyCode::Char(c), KeyModifiers::NONE);
        draw(&mut rig);
    }

    let parses = rig.app.state().workspace().active_parses();
    assert_eq!(parses.full, 1, "only the first parse is from scratch");
    assert_eq!(parses.incremental, 3);
}

#[test]
fn undo_is_reparsed_incrementally_too() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = app_with(dir.path(), "a.rs", "fn main() {}\n");
    draw(&mut rig);
    key(&mut rig, KeyCode::Char('x'), KeyModifiers::NONE);
    draw(&mut rig);

    key(&mut rig, KeyCode::Char('z'), KeyModifiers::CONTROL);
    let terminal = draw(&mut rig);

    assert_eq!(rig.app.state().workspace().active_parses().full, 1);
    assert_eq!(color_of(&terminal, "fn main"), Some(KEYWORD));
}

/// One frame, without waiting for the syntax worker.
fn draw_now(rig: &mut Rig) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, rig.app.state_mut()))
        .unwrap();
    terminal
}

#[test]
fn colors_stay_on_their_text_while_the_worker_is_still_parsing() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = app_with(dir.path(), "a.rs", "fn main() {}\n");
    draw(&mut rig);

    // Indent the line. The worker's answer is never taken in this test, so
    // what is on screen comes only from moving the old spans along.
    key(&mut rig, KeyCode::Home, KeyModifiers::NONE);
    for _ in 0..4 {
        key(&mut rig, KeyCode::Char(' '), KeyModifiers::NONE);
    }
    let terminal = draw_now(&mut rig);

    assert_eq!(color_of(&terminal, "fn main"), Some(KEYWORD));
}

#[test]
fn a_document_that_is_closed_is_forgotten_by_the_worker() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = app_with(dir.path(), "a.rs", "fn main() {}\n");
    draw(&mut rig);

    key(&mut rig, KeyCode::Char('w'), KeyModifiers::CONTROL);
    let terminal = draw(&mut rig);

    assert!(!rig.app.state().workspace().highlights_pending());
    drop(terminal);
}
