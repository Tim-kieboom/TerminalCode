use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::buffer::Position;
use crate::components::prepare_and_render;

const TEXT: &str = "alpha beta\nBeta gamma\ndelta beta\n";

fn key(code: KeyCode, modifiers: KeyModifiers) -> InputEvent {
    InputEvent::Key(KeyEvent::new(code, modifiers))
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_input(key(code, KeyModifiers::NONE));
}

fn ctrl(app: &mut App, c: char) {
    app.handle_input(key(KeyCode::Char(c), KeyModifiers::CONTROL));
}

fn alt(app: &mut App, c: char) {
    app.handle_input(key(KeyCode::Char(c), KeyModifiers::ALT));
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn app_with(text: &str) -> App {
    let mut app = App::default();
    app.handle_input(InputEvent::Paste(text.to_owned()));
    app.handle_input(key(KeyCode::Home, KeyModifiers::CONTROL));
    app
}

fn screen(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    terminal.backend().to_string()
}

fn selection(app: &App) -> (Position, Position) {
    let selection = *app.state().editor().selections().primary();
    (selection.start(), selection.end())
}

fn text(app: &App) -> String {
    app.state().editor().buffer().text()
}

#[test]
fn ctrl_f_opens_the_bar_and_typing_selects_the_first_match_from_the_cursor() {
    let mut app = app_with(TEXT);

    ctrl(&mut app, 'f');
    type_str(&mut app, "beta");

    assert!(app.state().find().is_some());
    assert_eq!(selection(&app), (Position::new(0, 6), Position::new(0, 10)));
}

#[test]
fn enter_steps_to_the_next_match_and_wraps() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');
    type_str(&mut app, "beta");

    press(&mut app, KeyCode::Enter);
    assert_eq!(selection(&app).0, Position::new(1, 0));
    press(&mut app, KeyCode::Enter);
    assert_eq!(selection(&app).0, Position::new(2, 6));
    press(&mut app, KeyCode::Enter);
    assert_eq!(selection(&app).0, Position::new(0, 6), "wrapped around");
}

#[test]
fn shift_enter_and_up_step_backwards() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');
    type_str(&mut app, "beta");

    app.handle_input(key(KeyCode::Enter, KeyModifiers::SHIFT));
    assert_eq!(selection(&app).0, Position::new(2, 6), "wrapped backwards");
    press(&mut app, KeyCode::Up);
    assert_eq!(selection(&app).0, Position::new(1, 0));
}

#[test]
fn escape_closes_the_bar_and_leaves_the_match_selected() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');
    type_str(&mut app, "gamma");

    press(&mut app, KeyCode::Esc);

    assert!(app.state().find().is_none());
    assert_eq!(selection(&app), (Position::new(1, 5), Position::new(1, 10)));
    assert_eq!(text(&app), TEXT, "nothing was typed into the document");
}

#[test]
fn typing_in_the_bar_does_not_edit_the_document() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');

    type_str(&mut app, "abc");
    ctrl(&mut app, 'z');
    ctrl(&mut app, 'q');

    assert_eq!(text(&app), TEXT);
    assert!(!app.should_quit());
}

#[test]
fn match_case_and_regex_toggle_while_the_bar_is_open() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');
    type_str(&mut app, "Beta");
    assert_eq!(app.state().find().unwrap().matches().len(), 3);

    alt(&mut app, 'c');
    assert_eq!(app.state().find().unwrap().matches().len(), 1);
    assert_eq!(selection(&app).0, Position::new(1, 0));

    alt(&mut app, 'c');
    alt(&mut app, 'r');
    type_str(&mut app, "|delta");
    assert_eq!(app.state().find().unwrap().matches().len(), 4);
}

#[test]
fn a_selected_word_becomes_the_query() {
    let mut app = app_with(TEXT);
    for _ in 0..6 {
        press(&mut app, KeyCode::Right);
    }
    for _ in 0..4 {
        app.handle_input(key(KeyCode::Right, KeyModifiers::SHIFT));
    }

    ctrl(&mut app, 'f');

    assert_eq!(app.state().find().unwrap().query(), "beta");
    assert_eq!(
        selection(&app).0,
        Position::new(0, 6),
        "the selected word is the first match"
    );
}

#[test]
fn the_last_query_comes_back_when_the_bar_is_opened_again() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');
    alt(&mut app, 'c');
    type_str(&mut app, "delta");
    press(&mut app, KeyCode::Esc);

    ctrl(&mut app, 'f');

    let find = app.state().find().unwrap();
    assert_eq!(find.query(), "delta");
    assert!(find.options().case_sensitive);
}

#[test]
fn the_bar_takes_a_row_from_the_editor_and_shows_the_count() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');
    type_str(&mut app, "beta");

    let screen = screen(&mut app, 100, 20);

    assert!(screen.contains("Find: beta"), "{screen}");
    assert!(screen.contains("1 of 3"), "{screen}");
    assert!(screen.contains("[ ] case (alt+c)"), "{screen}");
    assert!(screen.contains("[ ] regex (alt+r)"), "{screen}");
}

#[test]
fn no_matches_and_an_invalid_pattern_are_said_in_the_bar() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');
    type_str(&mut app, "zzz");
    assert!(screen(&mut app, 100, 20).contains("no results"));

    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Backspace);
    alt(&mut app, 'r');
    type_str(&mut app, "(beta");

    assert!(screen(&mut app, 100, 20).contains("invalid pattern"));
}

#[test]
fn closing_the_bar_gives_the_row_back() {
    let mut app = app_with(&"line\n".repeat(30));
    let before = screen(&mut app, 100, 30);
    ctrl(&mut app, 'f');
    let during = screen(&mut app, 100, 30);
    press(&mut app, KeyCode::Esc);
    let after = screen(&mut app, 100, 30);

    let rows = |screen: &str| screen.lines().filter(|row| row.contains("line")).count();
    assert_eq!(rows(&during) + 1, rows(&before));
    assert_eq!(rows(&after), rows(&before));
}

#[test]
fn a_match_far_down_scrolls_into_view_above_the_bar() {
    let mut text = "filler\n".repeat(40);
    text.push_str("the needle is here\n");
    let mut app = app_with(&text);
    ctrl(&mut app, 'f');

    type_str(&mut app, "needle");

    let screen = screen(&mut app, 100, 30);
    assert!(screen.contains("the needle is here"), "{screen}");
    assert!(screen.contains("Find: needle"), "{screen}");
}

#[test]
fn every_match_on_screen_is_highlighted_not_just_the_current_one() {
    use ratatui::style::Color;
    let mut app = app_with("beta one\nbeta two\n");
    ctrl(&mut app, 'f');
    type_str(&mut app, "beta");
    let mut terminal = Terminal::new(TestBackend::new(80, 30)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();

    let highlighted = |row: u16| {
        (0..80)
            .filter(|column| buffer[(*column, row)].bg == Color::Yellow)
            .count()
    };
    // The default theme paints matches on a yellow background. Find the rows
    // that hold the two lines of text.
    let rows: Vec<u16> = (0..30)
        .filter(|row| {
            (0..80)
                .map(|column| buffer[(column, *row)].symbol().to_owned())
                .collect::<String>()
                .contains("beta two")
        })
        .collect();
    assert_eq!(rows.len(), 1, "the line with the second match");
    assert_eq!(highlighted(rows[0]), 4, "the other match is highlighted");
}

#[test]
fn the_bar_takes_the_mouse_and_paste() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');

    app.handle_input(InputEvent::Paste("pasted".to_owned()));
    app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 40,
        row: 5,
        modifiers: KeyModifiers::NONE,
    }));

    assert_eq!(text(&app), TEXT);
    assert!(app.state().find().is_some());
}

#[test]
fn find_from_the_palette_and_its_key_is_listed() {
    let mut app = app_with(TEXT);

    press(&mut app, KeyCode::F(1));
    type_str(&mut app, "find in file");

    let entry = app.state().palette().unwrap().visible().next().unwrap();
    assert_eq!(entry.title, "Search: Find in File");
    assert_eq!(entry.keys.as_deref(), Some("ctrl+f"));
    press(&mut app, KeyCode::Enter);
    assert!(app.state().find().is_some());
}

#[test]
fn find_does_nothing_with_no_open_file() {
    let mut app = App::default();
    ctrl(&mut app, 'w');

    ctrl(&mut app, 'f');

    assert!(app.state().find().is_none());
}

#[test]
fn a_tiny_terminal_does_not_panic() {
    let mut app = app_with(TEXT);
    ctrl(&mut app, 'f');
    type_str(&mut app, "beta");

    for (width, height) in [(1, 1), (5, 2), (20, 3), (40, 4)] {
        screen(&mut app, width, height);
    }
}
