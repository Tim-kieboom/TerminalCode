//! A view or a click can outlive the layout or text it was made for: another
//! pane shortens the document, the file is rewritten outside, a tab closes
//! between two frames.

use std::fs;

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::initial_state;

fn key(code: KeyCode, modifiers: KeyModifiers) -> InputEvent {
    InputEvent::Key(KeyEvent::new(code, modifiers))
}

fn ctrl(app: &mut App, c: char) {
    app.handle_input(key(KeyCode::Char(c), KeyModifiers::CONTROL));
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> InputEvent {
    InputEvent::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

fn draw(app: &mut App) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    terminal
        .backend()
        .to_string()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// The text shown in the right half of the screen, where the second pane is
/// after a split.
fn right_half(rows: &[String]) -> String {
    rows.iter()
        .map(|row| row.chars().skip(75).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// `row 001` to `row N`, one per line.
fn numbered_lines(count: usize) -> String {
    (1..=count).map(|n| format!("row {n:03}\n")).collect()
}

/// Two panes on one document of 300 lines, the right one scrolled to the end.
fn two_panes_the_right_one_at_the_end() -> App {
    let mut app = App::default();
    app.handle_input(InputEvent::Paste(numbered_lines(300)));
    app.handle_input(key(KeyCode::Home, KeyModifiers::CONTROL));
    draw(&mut app);
    ctrl(&mut app, 'k');
    app.handle_input(key(KeyCode::Right, KeyModifiers::NONE));
    app.handle_input(key(KeyCode::End, KeyModifiers::CONTROL));
    draw(&mut app);
    app
}

#[test]
fn a_pane_scrolled_to_the_end_shows_the_text_after_another_pane_deletes_it() {
    let mut app = two_panes_the_right_one_at_the_end();
    assert!(
        !right_half(&draw(&mut app)).contains("line"),
        "scrolled past the first lines"
    );

    app.handle_input(key(KeyCode::Char('h'), KeyModifiers::ALT));
    ctrl(&mut app, 'a');
    app.handle_input(key(KeyCode::Delete, KeyModifiers::NONE));
    app.handle_input(InputEvent::Paste("fresh text".to_owned()));

    let right = right_half(&draw(&mut app));
    assert!(
        right.contains("fresh text"),
        "the right pane is blank:\n{right}"
    );
}

#[test]
fn a_pane_scrolled_to_the_end_shows_the_text_after_the_file_shrinks_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.txt");
    fs::write(&path, "old line\n".repeat(300)).unwrap();
    let mut app = App::new(initial_state(std::slice::from_ref(&path)).unwrap());
    draw(&mut app);
    app.handle_input(key(KeyCode::End, KeyModifiers::CONTROL));
    draw(&mut app);

    fs::write(&path, "short file\nof two lines\n").unwrap();
    app.handle_event(Event::FilesChanged(vec![path]));

    let rows = draw(&mut app);
    assert!(
        rows.iter().any(|row| row.contains("short file")),
        "the editor is blank after the reload:\n{}",
        rows.join("\n")
    );
}

#[test]
fn wheel_scrolling_is_not_undone_by_the_next_frame() {
    let mut app = App::default();
    app.handle_input(InputEvent::Paste("line\n".repeat(300)));
    app.handle_input(key(KeyCode::Home, KeyModifiers::CONTROL));
    draw(&mut app);

    for _ in 0..5 {
        app.handle_input(mouse(MouseEventKind::ScrollDown, 60, 10));
    }
    draw(&mut app);

    let top = app.state().editor().scroll().top;
    assert!(top > 0, "scrolled down by the wheel");
    draw(&mut app);
    assert_eq!(
        app.state().editor().scroll().top,
        top,
        "stays where the wheel put it"
    );
}

fn three_tabs() -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<_> = ["a", "b", "c"]
        .iter()
        .map(|name| {
            let path = dir.path().join(name);
            fs::write(&path, "x").unwrap();
            path
        })
        .collect();
    (App::new(initial_state(&paths).unwrap()), dir)
}

#[test]
fn a_middle_click_on_the_place_of_a_tab_that_just_closed_does_nothing() {
    let (mut app, _dir) = three_tabs();
    draw(&mut app);
    let third = app.state().workspace().tab_areas()[2];
    app.state_mut().workspace_mut().activate_tab(2);
    ctrl(&mut app, 'w');

    // No frame is drawn between the close and the click.
    app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Middle),
        third.x + 1,
        third.y,
    ));

    assert_eq!(app.state().workspace().tab_names().0.len(), 2);
}

#[test]
fn a_click_on_the_place_of_a_tab_that_just_closed_leaves_a_valid_active_tab() {
    let (mut app, _dir) = three_tabs();
    draw(&mut app);
    let third = app.state().workspace().tab_areas()[2];
    app.state_mut().workspace_mut().activate_tab(2);
    ctrl(&mut app, 'w');

    app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Left),
        third.x + 1,
        third.y,
    ));
    app.handle_input(InputEvent::Paste("typed".to_owned()));

    let (names, active) = app.state().workspace().tab_names();
    assert!(
        active < names.len(),
        "active tab {active} of {}",
        names.len()
    );
    assert!(app.state().editor().buffer().text().contains("typed"));
}
