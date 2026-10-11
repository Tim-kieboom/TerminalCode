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
use crate::components::finder::Walk;
use crate::components::prepare_and_render;
use crate::event::Event;

fn key(code: KeyCode, modifiers: KeyModifiers) -> InputEvent {
    InputEvent::Key(KeyEvent::new(code, modifiers))
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_input(key(code, KeyModifiers::NONE));
}

fn ctrl(app: &mut App, c: char) {
    app.handle_input(key(KeyCode::Char(c), KeyModifiers::CONTROL));
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
        ("Cargo.toml", "[package]\n"),
        ("src/main.rs", "fn main() {}\n"),
        ("src/lib.rs", "// lib\n"),
        ("docs/design.md", "# design\n"),
        ("target/debug/junk.rs", "ignored\n"),
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

/// Delivers what the walk found, up to the end of the walk the finder listens to.
fn finish_walk(app: &mut App, rx: &mut mpsc::Receiver<Event>) {
    while let Some(event) = rx.blocking_recv() {
        app.handle_event(event);
        let over = app
            .state()
            .finder()
            .is_some_and(|finder| finder.walk() != Walk::Running);
        if over {
            return;
        }
    }
}

/// Delivers events until the finder has at least one file.
fn first_batch(app: &mut App, rx: &mut mpsc::Receiver<Event>) {
    while let Some(event) = rx.blocking_recv() {
        app.handle_event(event);
        if app
            .state()
            .finder()
            .is_some_and(|finder| finder.file_count() > 0)
        {
            return;
        }
    }
}

fn results(app: &App) -> Vec<String> {
    app.state()
        .finder()
        .unwrap()
        .results()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn screen(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    terminal.backend().to_string()
}

#[test]
fn ctrl_p_opens_the_finder_and_it_lists_the_projects_files_without_the_ignored_ones() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());

    ctrl(&mut app, 'p');
    finish_walk(&mut app, &mut rx);

    let mut found = results(&app);
    found.sort();
    assert_eq!(
        found,
        [
            ".gitignore",
            "Cargo.toml",
            "docs/design.md",
            "src/lib.rs",
            "src/main.rs"
        ]
    );
}

#[test]
fn typing_narrows_the_list_and_enter_opens_the_file_in_the_editor() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    ctrl(&mut app, 'p');
    finish_walk(&mut app, &mut rx);

    type_str(&mut app, "main");
    assert_eq!(results(&app), ["src/main.rs"]);
    press(&mut app, KeyCode::Enter);

    assert!(app.state().finder().is_none());
    assert_eq!(app.state().editor().display_name(), "main.rs");
    assert_eq!(app.state().editor().buffer().text(), "fn main() {}\n");
}

#[test]
fn the_editor_gets_the_keyboard_even_if_the_explorer_had_it() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    ctrl(&mut app, 'b');
    ctrl(&mut app, 'p');
    finish_walk(&mut app, &mut rx);
    type_str(&mut app, "lib");

    press(&mut app, KeyCode::Enter);

    assert_eq!(app.state().focus(), Focus::Editor);
}

#[test]
fn partial_results_show_up_while_the_walk_is_still_running() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    ctrl(&mut app, 'p');
    // Typed before a single file has arrived.
    type_str(&mut app, "main");
    assert!(results(&app).is_empty());
    let before = screen(&mut app);
    assert!(
        before.contains("looking for files") || before.contains("indexing"),
        "{before}"
    );

    first_batch(&mut app, &mut rx);

    assert_eq!(results(&app), ["src/main.rs"]);
    assert!(screen(&mut app).contains("main.rs"));
}

#[test]
fn the_footer_counts_files_while_indexing_and_when_done() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    ctrl(&mut app, 'p');
    assert!(screen(&mut app).contains("indexing... 0 files"));

    finish_walk(&mut app, &mut rx);

    let done = screen(&mut app);
    assert!(done.contains(" 5 files"), "{done}");
    assert!(!done.contains("indexing"), "{done}");
}

#[test]
fn no_matching_file_says_so_once_the_walk_is_done() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    ctrl(&mut app, 'p');
    finish_walk(&mut app, &mut rx);

    type_str(&mut app, "zzzz");

    assert!(screen(&mut app).contains("no matching file"));
}

#[test]
fn enter_with_nothing_selected_keeps_the_finder_open() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    ctrl(&mut app, 'p');
    finish_walk(&mut app, &mut rx);
    type_str(&mut app, "zzzz");

    press(&mut app, KeyCode::Enter);

    assert!(app.state().finder().is_some());
}

#[test]
fn escape_closes_the_finder_without_opening_anything() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    ctrl(&mut app, 'p');
    finish_walk(&mut app, &mut rx);
    let tabs = app.state().workspace().tab_names().0.len();

    press(&mut app, KeyCode::Esc);

    assert!(app.state().finder().is_none());
    assert_eq!(app.state().workspace().tab_names().0.len(), tabs);
}

#[test]
fn a_file_that_is_already_open_is_switched_to_not_opened_twice() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    for _ in 0..2 {
        ctrl(&mut app, 'p');
        finish_walk(&mut app, &mut rx);
        type_str(&mut app, "lib");
        press(&mut app, KeyCode::Enter);
    }

    assert_eq!(
        app.state().workspace().document_count(),
        2,
        "the empty tab and lib.rs"
    );
}

#[test]
fn the_finder_takes_the_keys_the_mouse_and_paste() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    ctrl(&mut app, 'p');
    finish_walk(&mut app, &mut rx);

    type_str(&mut app, "x");
    app.handle_input(InputEvent::Paste("pasted".to_owned()));
    app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 60,
        row: 15,
        modifiers: KeyModifiers::NONE,
    }));
    ctrl(&mut app, 'q');

    assert_eq!(app.state().editor().buffer().text(), "");
    assert!(!app.should_quit());
    assert!(app.state().finder().is_some());
}

#[test]
fn files_from_an_old_walk_are_ignored() {
    let dir = project();
    let (mut app, _rx) = app_in(dir.path());
    ctrl(&mut app, 'p');

    app.handle_event(Event::FinderBatch {
        scan: 999,
        files: vec!["stale.rs".into()],
    });
    app.handle_event(Event::FinderDone {
        scan: 999,
        unreadable: 0,
    });

    assert_eq!(app.state().finder().unwrap().file_count(), 0);
}

#[test]
fn results_arriving_after_the_finder_closed_are_dropped_quietly() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    ctrl(&mut app, 'p');
    press(&mut app, KeyCode::Esc);

    finish_walk_or_close(&mut app, &mut rx);

    assert!(app.state().finder().is_none());
}

/// Delivers whatever arrives until the walk ends or its channel closes.
fn finish_walk_or_close(app: &mut App, rx: &mut mpsc::Receiver<Event>) {
    while let Ok(event) = rx.try_recv() {
        app.handle_event(event);
    }
}

#[test]
fn without_the_event_loop_opening_the_finder_is_an_error() {
    let mut app = App::default();

    ctrl(&mut app, 'p');

    assert!(app.state().finder().is_none());
    assert!(app.state().notifications().has_errors());
}

#[test]
fn a_second_open_starts_a_fresh_walk_and_ignores_the_first() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    ctrl(&mut app, 'p');
    press(&mut app, KeyCode::Esc);
    ctrl(&mut app, 'p');

    // Both walks send to the same channel; only the second one's files count.
    finish_walk(&mut app, &mut rx);
    finish_walk_or_close(&mut app, &mut rx);

    assert_eq!(app.state().finder().unwrap().file_count(), 5);
}

#[test]
fn the_palette_lists_go_to_file_with_its_key() {
    let mut app = App::default();

    press(&mut app, KeyCode::F(1));
    type_str(&mut app, "go to file");

    let entry = app.state().palette().unwrap().visible().next().unwrap();
    assert_eq!(entry.title, "File: Go to File");
    assert_eq!(entry.keys.as_deref(), Some("ctrl+p"));
}

#[test]
fn the_finder_works_from_the_palette_too() {
    let dir = project();
    let (mut app, mut rx) = app_in(dir.path());
    press(&mut app, KeyCode::F(1));
    type_str(&mut app, "go to file");
    press(&mut app, KeyCode::Enter);

    finish_walk(&mut app, &mut rx);

    assert!(app.state().palette().is_none());
    assert_eq!(app.state().finder().unwrap().file_count(), 5);
}

/// Files whose names a path-as-text conversion would damage: a backslash in a
/// Unix file name, and a name that is not valid Unicode.
#[cfg(target_os = "linux")]
mod unusual_names {
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::ffi::OsStringExt;

    use crossterm::event::KeyCode;

    use super::{app_in, ctrl, finish_walk, press, type_str};

    fn project_with_unusual_names() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("we\\ird_name.txt"), "backslash file\n").unwrap();
        let latin = OsString::from_vec(b"caf\xe9_menu.txt".to_vec());
        fs::write(dir.path().join(latin), "latin file\n").unwrap();
        fs::write(dir.path().join("plain.txt"), "plain file\n").unwrap();
        dir
    }

    #[test]
    fn a_file_with_a_backslash_in_its_name_opens() {
        let dir = project_with_unusual_names();
        let (mut app, mut rx) = app_in(dir.path());
        ctrl(&mut app, 'p');
        finish_walk(&mut app, &mut rx);

        type_str(&mut app, "ird_name");
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.state().editor().buffer().text(), "backslash file\n");
        assert!(app.state().notifications().latest().is_none());
    }

    #[test]
    fn a_file_whose_name_is_not_valid_unicode_opens() {
        let dir = project_with_unusual_names();
        let (mut app, mut rx) = app_in(dir.path());
        ctrl(&mut app, 'p');
        finish_walk(&mut app, &mut rx);

        type_str(&mut app, "menu");
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.state().editor().buffer().text(), "latin file\n");
        assert!(!app.state().notifications().has_errors());
    }
}

// ---- a place in the file, a split, recent files, highlighted matches

fn long_project() -> tempfile::TempDir {
    let dir = project();
    fs::write(
        dir.path().join("src/main.rs"),
        "line one\nline two\nline three, longer\nline four\n",
    )
    .unwrap();
    dir
}

fn finder_ready(dir: &Path) -> (App, mpsc::Receiver<Event>) {
    let (mut app, mut rx) = app_in(dir);
    ctrl(&mut app, 'p');
    finish_walk(&mut app, &mut rx);
    (app, rx)
}

fn head(app: &App) -> (usize, usize) {
    let head = app.state().editor().selections().primary().head();
    (head.line, head.column)
}

#[test]
fn a_line_after_the_name_opens_the_file_at_that_line() {
    let dir = long_project();
    let (mut app, _rx) = finder_ready(dir.path());

    type_str(&mut app, "main.rs:3");
    press(&mut app, KeyCode::Enter);

    assert_eq!(app.state().editor().display_name(), "main.rs");
    assert_eq!(head(&app), (2, 0));
}

#[test]
fn a_column_puts_the_cursor_there() {
    let dir = long_project();
    let (mut app, _rx) = finder_ready(dir.path());

    type_str(&mut app, "main.rs:3:6");
    press(&mut app, KeyCode::Enter);

    assert_eq!(head(&app), (2, 5));
}

#[test]
fn a_line_past_the_end_goes_to_the_last_line() {
    let dir = long_project();
    let (mut app, _rx) = finder_ready(dir.path());

    type_str(&mut app, "main.rs:999:999");
    press(&mut app, KeyCode::Enter);

    let (line, _) = head(&app);
    assert_eq!(line, app.state().editor().buffer().len_lines() - 1);
}

#[test]
fn a_file_that_is_already_open_is_moved_to_the_line() {
    let dir = long_project();
    let (mut app, mut rx) = finder_ready(dir.path());
    type_str(&mut app, "main.rs:4");
    press(&mut app, KeyCode::Enter);
    assert_eq!(head(&app), (3, 0));
    let tabs = app.state().workspace().tab_names().0.len();

    ctrl(&mut app, 'p');
    finish_walk(&mut app, &mut rx);
    type_str(&mut app, "main.rs:2");
    press(&mut app, KeyCode::Enter);

    assert_eq!(app.state().workspace().tab_names().0.len(), tabs);
    assert_eq!(head(&app), (1, 0));
}

#[test]
fn the_footer_says_where_the_file_will_open() {
    let dir = long_project();
    let (mut app, _rx) = finder_ready(dir.path());

    type_str(&mut app, "main.rs:3:2");

    assert!(
        screen(&mut app).contains("line 3, column 2"),
        "{}",
        screen(&mut app)
    );
}

#[test]
fn alt_enter_opens_the_file_in_a_new_pane() {
    let dir = long_project();
    let (mut app, _rx) = finder_ready(dir.path());
    let panes = app.state().workspace().pane_count();
    type_str(&mut app, "lib");

    app.handle_input(key(KeyCode::Enter, KeyModifiers::ALT));

    assert_eq!(app.state().workspace().pane_count(), panes + 1);
    assert_eq!(app.state().editor().display_name(), "lib.rs");
    assert_eq!(app.state().workspace().tab_names().0, ["lib.rs"]);
    assert_eq!(app.state().focus(), Focus::Editor);
    assert!(app.state().finder().is_none());
}

#[test]
fn the_pane_the_split_came_from_is_left_as_it_was() {
    let dir = long_project();
    let (mut app, _rx) = finder_ready(dir.path());
    let before = app.state().workspace().tab_names();
    type_str(&mut app, "lib");
    app.handle_input(key(KeyCode::Enter, KeyModifiers::ALT));

    app.state_mut().workspace_mut().focus_next_pane();

    assert_eq!(app.state().workspace().tab_names(), before);
}

#[test]
fn alt_enter_with_a_place_goes_there_in_the_new_pane() {
    let dir = long_project();
    let (mut app, _rx) = finder_ready(dir.path());
    type_str(&mut app, "main.rs:2:3");

    app.handle_input(key(KeyCode::Enter, KeyModifiers::ALT));

    assert_eq!(app.state().editor().display_name(), "main.rs");
    assert_eq!(head(&app), (1, 2));
}

#[test]
fn alt_enter_on_a_file_that_is_already_open_shows_it_in_the_new_pane_too() {
    let dir = long_project();
    let (mut app, mut rx) = finder_ready(dir.path());
    type_str(&mut app, "lib");
    press(&mut app, KeyCode::Enter);
    let documents = app.state().workspace().document_count();
    ctrl(&mut app, 'p');
    finish_walk(&mut app, &mut rx);
    type_str(&mut app, "lib");

    app.handle_input(key(KeyCode::Enter, KeyModifiers::ALT));

    assert_eq!(app.state().workspace().document_count(), documents);
    assert_eq!(app.state().workspace().pane_count(), 2);
    assert_eq!(app.state().editor().display_name(), "lib.rs");
}

#[test]
fn alt_enter_on_a_file_that_cannot_be_read_changes_no_pane() {
    let dir = long_project();
    let (mut app, _rx) = finder_ready(dir.path());
    fs::remove_file(dir.path().join("src/lib.rs")).unwrap();
    type_str(&mut app, "lib");

    app.handle_input(key(KeyCode::Enter, KeyModifiers::ALT));

    assert_eq!(app.state().workspace().pane_count(), 1);
    assert!(app.state().notifications().has_errors());
}

#[test]
fn a_file_opened_lately_comes_first_the_next_time() {
    let dir = long_project();
    let (mut app, mut rx) = finder_ready(dir.path());
    type_str(&mut app, "design");
    press(&mut app, KeyCode::Enter);

    ctrl(&mut app, 'p');
    finish_walk(&mut app, &mut rx);

    assert_eq!(results(&app)[0], "docs/design.md");
}

#[test]
fn files_opened_from_the_explorer_count_as_recent_too() {
    let dir = long_project();
    let (mut app, mut rx) = app_in(dir.path());
    app.state_mut()
        .workspace_mut()
        .open_path(&dir.path().join("src/lib.rs"))
        .unwrap();

    ctrl(&mut app, 'p');
    finish_walk(&mut app, &mut rx);

    assert_eq!(results(&app)[0], "src/lib.rs");
}

#[test]
fn the_matched_characters_are_drawn_differently() {
    let dir = long_project();
    let (mut app, _rx) = finder_ready(dir.path());
    type_str(&mut app, "mn");

    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let theme = app.state().theme();
    let hit = theme.style("finder.match");
    let mut found = false;
    for y in 0..30 {
        let row: String = (0..100)
            .map(|x| buffer[(x, y)].symbol().to_owned())
            .collect();
        let Some(byte) = row.find("main.rs") else {
            continue;
        };
        let x = row[..byte].chars().count() as u16;
        // `m` and `n` are matched; `a` and `i` are not.
        assert_eq!(buffer[(x, y)].style().fg, hit.fg, "m");
        assert_eq!(buffer[(x + 3, y)].style().fg, hit.fg, "n");
        assert_ne!(buffer[(x + 1, y)].style().fg, hit.fg, "a");
        found = true;
    }
    assert!(found, "{}", terminal.backend());
}
