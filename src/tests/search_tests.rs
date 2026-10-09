use std::fs;
use std::path::Path;

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::app::App;
use crate::buffer::Position;
use crate::components::prepare_and_render;
use crate::components::search::Status;
use crate::event::Event;
use crate::state::{AppState, Focus};

fn key(code: KeyCode, modifiers: KeyModifiers) -> InputEvent {
    InputEvent::Key(KeyEvent::new(code, modifiers))
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_input(key(code, KeyModifiers::NONE));
}

fn alt(app: &mut App, c: char) {
    app.handle_input(key(KeyCode::Char(c), KeyModifiers::ALT));
}

fn type_str(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in [
        (".gitignore", "target/\n"),
        ("src/main.rs", "fn main() {\n    println!(\"Hello\");\n}\n"),
        ("src/lib.rs", "// hello there\npub fn hello() {}\n"),
        ("docs/notes.md", "nothing to see\n"),
        ("target/junk.rs", "hello ignored\n"),
    ] {
        let full = dir.path().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    }
    dir
}

fn app_in(root: &Path) -> (App, mpsc::Receiver<Event>) {
    let mut state = AppState::default();
    state.open_project(root).unwrap();
    let (tx, rx) = mpsc::channel(64);
    (App::new(state).with_events(tx), rx)
}

fn open(app: &mut App) {
    alt(app, 'f');
}

/// Delivers what the search finds until it is over (or found unusable).
fn finish(app: &mut App, rx: &mut mpsc::Receiver<Event>) {
    while let Some(event) = rx.blocking_recv() {
        app.handle_event(event);
        let over = app
            .state()
            .search()
            .is_some_and(|search| !matches!(search.status(), Status::Running));
        if over {
            return;
        }
    }
}

fn found(app: &App) -> Vec<String> {
    app.state()
        .search()
        .unwrap()
        .hits()
        .iter()
        .map(|hit| format!("{}:{}", hit.path.display(), hit.line + 1))
        .collect()
}

fn screen(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    terminal.backend().to_string()
}

#[test]
fn alt_f_opens_the_search_and_typing_finds_matching_lines() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());

    open(&mut app);
    type_str(&mut app, "hello");
    finish(&mut app, &mut rx);

    assert_eq!(
        found(&app),
        ["src/lib.rs:1", "src/lib.rs:2", "src/main.rs:2"]
    );
}

#[test]
fn the_results_are_drawn_with_where_they_are_and_the_footer_counts_them() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    open(&mut app);
    type_str(&mut app, "hello");
    finish(&mut app, &mut rx);

    let screen = screen(&mut app);

    assert!(screen.contains("Find in Project"), "{screen}");
    assert!(screen.contains("> hello"), "{screen}");
    assert!(screen.contains("src/main.rs:2"), "{screen}");
    assert!(screen.contains("println!(\"Hello\");"), "{screen}");
    assert!(screen.contains("3 results in 2 files"), "{screen}");
    assert!(screen.contains("[ ] match case"), "{screen}");
}

#[test]
fn enter_opens_the_file_with_the_cursor_on_the_match_and_gives_the_editor_the_keyboard() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    press(&mut app, KeyCode::Esc);
    app.handle_input(key(KeyCode::Char('b'), KeyModifiers::CONTROL));
    open(&mut app);
    type_str(&mut app, "println");
    finish(&mut app, &mut rx);

    press(&mut app, KeyCode::Enter);

    assert!(app.state().search().is_none());
    assert_eq!(app.state().focus(), Focus::Editor);
    assert_eq!(app.state().editor().display_name(), "main.rs");
    assert_eq!(
        app.state().editor().selections().primary().head(),
        Position::new(1, 4)
    );
}

#[test]
fn the_arrows_choose_which_result_enter_opens() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    open(&mut app);
    type_str(&mut app, "hello");
    finish(&mut app, &mut rx);

    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);

    assert_eq!(app.state().editor().display_name(), "main.rs");
}

#[test]
fn match_case_and_regex_can_be_switched_on_while_searching() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    open(&mut app);
    type_str(&mut app, "Hello");
    finish(&mut app, &mut rx);
    assert_eq!(found(&app).len(), 3, "case is ignored by default");

    alt(&mut app, 'c');
    finish(&mut app, &mut rx);
    assert_eq!(found(&app), ["src/main.rs:2"]);
    assert!(screen(&mut app).contains("[x] match case"));

    alt(&mut app, 'c');
    alt(&mut app, 'r');
    type_str(&mut app, "|nothing");
    finish(&mut app, &mut rx);
    assert!(
        found(&app).contains(&"docs/notes.md:1".to_owned()),
        "{:?}",
        found(&app)
    );
    assert!(screen(&mut app).contains("[x] regex"));
}

#[test]
fn an_invalid_pattern_is_explained_and_fixing_it_searches_again() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    open(&mut app);
    alt(&mut app, 'r');
    type_str(&mut app, "(hello");

    assert!(app.state().search().unwrap().hits().is_empty());
    assert!(screen(&mut app).contains("invalid pattern"));

    press(&mut app, KeyCode::Backspace);
    // "(hello" minus its last letter is still invalid; closing the group fixes it.
    type_str(&mut app, "o)");
    finish(&mut app, &mut rx);
    assert_eq!(found(&app).len(), 3);
}

#[test]
fn typing_replaces_the_results_of_the_earlier_query() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    open(&mut app);
    type_str(&mut app, "hello");
    finish(&mut app, &mut rx);
    assert_eq!(found(&app).len(), 3);

    type_str(&mut app, " there");
    finish(&mut app, &mut rx);

    assert_eq!(found(&app), ["src/lib.rs:1"]);
}

#[test]
fn clearing_the_query_clears_the_results_and_stops_searching() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    open(&mut app);
    type_str(&mut app, "hello");
    finish(&mut app, &mut rx);

    app.handle_input(key(KeyCode::Char('u'), KeyModifiers::CONTROL));

    assert!(found(&app).is_empty());
    assert_eq!(app.state().search().unwrap().status(), &Status::Idle);
    assert!(screen(&mut app).contains("type to search"));
}

#[test]
fn reopening_the_search_restores_the_last_query_and_options_and_runs_it() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    open(&mut app);
    alt(&mut app, 'c');
    type_str(&mut app, "Hello");
    finish(&mut app, &mut rx);
    press(&mut app, KeyCode::Esc);
    assert!(app.state().search().is_none());

    open(&mut app);
    finish(&mut app, &mut rx);

    let search = app.state().search().unwrap();
    assert_eq!(search.query(), "Hello");
    assert!(search.options().case_sensitive);
    assert_eq!(found(&app), ["src/main.rs:2"]);
}

#[test]
fn no_results_says_so_and_enter_keeps_the_search_open() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    open(&mut app);
    type_str(&mut app, "zzzzzz");
    finish(&mut app, &mut rx);

    press(&mut app, KeyCode::Enter);

    assert!(app.state().search().is_some());
    assert!(screen(&mut app).contains("no results"));
}

#[test]
fn escape_closes_without_opening_anything() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    open(&mut app);
    type_str(&mut app, "hello");
    finish(&mut app, &mut rx);
    let tabs = app.state().workspace().tab_names().0.len();

    press(&mut app, KeyCode::Esc);

    assert!(app.state().search().is_none());
    assert_eq!(app.state().workspace().tab_names().0.len(), tabs);
}

#[test]
fn the_search_takes_the_keys_the_mouse_and_paste() {
    let dir = project();
    let (mut app, _rx) = app_in(dir.path());
    open(&mut app);

    app.handle_input(InputEvent::Paste("pasted".to_owned()));
    app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 60,
        row: 15,
        modifiers: KeyModifiers::NONE,
    }));
    app.handle_input(key(KeyCode::Char('q'), KeyModifiers::CONTROL));

    assert_eq!(app.state().editor().buffer().text(), "");
    assert!(!app.should_quit());
    assert!(app.state().search().is_some());
}

#[test]
fn results_of_an_old_search_are_ignored() {
    let dir = project();
    let (mut app, _rx) = app_in(dir.path());
    open(&mut app);
    type_str(&mut app, "hello");

    app.handle_event(Event::SearchBatch {
        search: 999,
        hits: vec![crate::components::search::Hit::new(
            "stale.rs".into(),
            1,
            "x",
            0..1,
        )],
    });
    app.handle_event(Event::SearchDone {
        search: 999,
        files: 1,
        truncated: false,
    });

    assert!(found(&app).is_empty());
    assert_eq!(app.state().search().unwrap().status(), &Status::Running);
}

#[test]
fn results_arriving_after_the_search_closed_are_dropped_quietly() {
    let dir = project();
    let (mut app, _rx) = app_in(dir.path());
    open(&mut app);
    press(&mut app, KeyCode::Esc);

    app.handle_event(Event::SearchDone {
        search: 1,
        files: 0,
        truncated: false,
    });

    assert!(app.state().search().is_none());
}

#[test]
fn without_the_event_loop_opening_the_search_is_an_error() {
    let mut app = App::default();

    open(&mut app);

    assert!(app.state().search().is_none());
    assert!(app.state().notifications().has_errors());
}

#[test]
fn the_palette_lists_find_in_project_with_its_key() {
    let mut app = App::default();

    press(&mut app, KeyCode::F(1));
    type_str(&mut app, "find in project");

    let entry = app.state().palette().unwrap().visible().next().unwrap();
    assert_eq!(entry.title, "Search: Find in Project");
    assert_eq!(entry.keys.as_deref(), Some("alt+f"));
}

#[test]
fn opening_a_result_for_a_file_that_is_already_open_switches_to_it() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    for _ in 0..2 {
        open(&mut app);
        type_str(&mut app, "pub fn");
        finish(&mut app, &mut rx);
        press(&mut app, KeyCode::Enter);
    }

    assert_eq!(
        app.state().workspace().document_count(),
        2,
        "the empty tab and lib.rs"
    );
    assert_eq!(
        app.state().editor().selections().primary().head(),
        Position::new(1, 0)
    );
}
