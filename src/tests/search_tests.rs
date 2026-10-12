use std::fs;
use std::path::Path;

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::{AppState, Focus};
use crate::buffer::Position;
use crate::components::prepare_and_render;
use crate::components::search::Status;
use crate::event::Event;

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
        .map(|hit| {
            format!("{}:{}", hit.path.display(), hit.line + 1)
                .replace(std::path::MAIN_SEPARATOR, "/")
        })
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
    let place = format!("src{}main.rs:2", std::path::MAIN_SEPARATOR);
    assert!(screen.contains(&place), "{screen}");
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

// ---- stepping through the results without opening the search

fn f4(app: &mut App) {
    press(app, KeyCode::F(4));
}

fn shift_f4(app: &mut App) {
    app.handle_input(key(KeyCode::F(4), KeyModifiers::SHIFT));
}

/// Searches for `hello` and closes the search with escape.
fn searched_and_closed() -> (App, tempfile::TempDir) {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    open(&mut app);
    type_str(&mut app, "hello");
    finish(&mut app, &mut rx);
    press(&mut app, KeyCode::Esc);
    (app, dir)
}

fn place(app: &App) -> (String, usize) {
    let name = app.state().editor().display_name();
    (
        name,
        app.state().editor().selections().primary().head().line,
    )
}

#[test]
fn f4_goes_to_the_first_result_after_closing_the_search() {
    let (mut app, _dir) = searched_and_closed();

    f4(&mut app);

    // The results are in path order: src/lib.rs line 1 comes first.
    assert_eq!(place(&app), ("lib.rs".to_owned(), 0));
}

#[test]
fn f4_goes_on_to_the_next_result_and_wraps() {
    let (mut app, _dir) = searched_and_closed();

    f4(&mut app);
    let first = place(&app);
    f4(&mut app);
    let second = place(&app);
    f4(&mut app);
    let third = place(&app);
    f4(&mut app);

    assert_ne!(first, second);
    assert_ne!(second, third);
    assert_eq!(place(&app), first, "three results, then back to the first");
}

#[test]
fn shift_f4_goes_back() {
    let (mut app, _dir) = searched_and_closed();
    f4(&mut app);
    f4(&mut app);
    let second = place(&app);
    f4(&mut app);

    shift_f4(&mut app);

    assert_eq!(place(&app), second);
}

#[test]
fn shift_f4_from_the_start_wraps_to_the_last_result() {
    let (mut app, _dir) = searched_and_closed();
    f4(&mut app);
    let first = place(&app);

    shift_f4(&mut app);

    assert_ne!(place(&app), first);
    f4(&mut app);
    assert_eq!(place(&app), first);
}

#[test]
fn stepping_says_which_result_it_is_on() {
    let (mut app, _dir) = searched_and_closed();

    f4(&mut app);

    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("result 1 of 3")
    );
    f4(&mut app);
    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("result 2 of 3")
    );
}

#[test]
fn stepping_starts_after_the_result_that_was_chosen() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    open(&mut app);
    type_str(&mut app, "hello");
    finish(&mut app, &mut rx);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    let chosen = place(&app);

    f4(&mut app);

    assert_ne!(place(&app), chosen);
    shift_f4(&mut app);
    assert_eq!(place(&app), chosen);
}

#[test]
fn without_a_search_there_is_nothing_to_step_through() {
    let mut app = App::default();

    f4(&mut app);

    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("no search results")
    );
}

#[test]
fn a_search_that_found_nothing_says_so() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    open(&mut app);
    type_str(&mut app, "zzzzz");
    finish(&mut app, &mut rx);
    press(&mut app, KeyCode::Esc);

    f4(&mut app);

    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("found nothing")
    );
}

#[test]
fn a_result_whose_file_is_gone_is_an_error_not_a_crash() {
    let (mut app, dir) = searched_and_closed();
    fs::remove_file(dir.path().join("src/lib.rs")).unwrap();

    f4(&mut app);

    assert!(app.state().notifications().has_errors());
}

#[test]
fn the_palette_offers_the_steps() {
    use crate::event::action::Action;
    let actions = Action::palette_actions();
    assert!(actions.contains(&Action::NextSearchResult));
    assert!(actions.contains(&Action::PreviousSearchResult));
}

#[test]
fn stepping_gives_the_keyboard_to_the_editor() {
    let (mut app, _dir) = searched_and_closed();
    app.state_mut().set_focus(Focus::Explorer);

    f4(&mut app);

    assert_eq!(app.state().focus(), Focus::Editor);
}

#[test]
fn a_new_search_replaces_the_remembered_results() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    open(&mut app);
    type_str(&mut app, "hello");
    finish(&mut app, &mut rx);
    press(&mut app, KeyCode::Esc);
    open(&mut app);
    for _ in 0.."hello".len() {
        press(&mut app, KeyCode::Backspace);
    }
    type_str(&mut app, "nothing");
    finish(&mut app, &mut rx);
    press(&mut app, KeyCode::Esc);

    f4(&mut app);

    assert_eq!(place(&app), ("notes.md".to_owned(), 0));
}

#[test]
fn after_escape_the_highlighted_result_is_the_next_one() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    open(&mut app);
    type_str(&mut app, "hello");
    finish(&mut app, &mut rx);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Esc);

    f4(&mut app);

    assert_eq!(place(&app), ("lib.rs".to_owned(), 1), "the second result");
}

#[test]
fn after_escape_shift_f4_goes_to_the_result_before_the_highlighted_one() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    open(&mut app);
    type_str(&mut app, "hello");
    finish(&mut app, &mut rx);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Esc);

    shift_f4(&mut app);

    assert_eq!(place(&app), ("lib.rs".to_owned(), 0), "the first result");
}
