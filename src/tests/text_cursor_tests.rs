//! The text cursor in the palette, the file finder, the project search and the find bar:
//! `left`, `right`, `home`, `end`, `ctrl+left`, `ctrl+right` move it, `delete` and backspace edit
//! at it, a letter is typed at it, and what the text matches follows every edit.

use std::fs;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::AppState;
use crate::buffer::Buffer;
use crate::components::editor::Editor;
use crate::components::popups::finder::Walk;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::event::action::Action;

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    _dir: tempfile::TempDir,
}

fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("alpha.txt"),
        "alpha text\nfind me here\nand here\n",
    )
    .unwrap();
    fs::write(dir.path().join("beta.txt"), "beta\n").unwrap();
    fs::write(dir.path().join("gamma.rs"), "gamma\n").unwrap();
    let mut state = AppState::new(Editor::new(Buffer::default()));
    state.open_project(dir.path()).unwrap();
    let (tx, events) = mpsc::channel(256);
    let mut app = App::new(state).with_events(tx);
    app.state_mut()
        .workspace_mut()
        .open_path(&dir.path().join("alpha.txt"))
        .unwrap();
    Rig {
        app,
        events,
        _dir: dir,
    }
}

impl Rig {
    fn press(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
    }

    fn key(&mut self, code: KeyCode) {
        self.press(code, KeyModifiers::NONE);
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.key(KeyCode::Char(c));
        }
    }

    /// Takes in what the background work sent, until the finder's walk is over (when one is
    /// open).
    fn finish_walk(&mut self) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            while let Ok(event) = self.events.try_recv() {
                self.app.handle_event(event);
            }
            let done = self
                .app
                .state()
                .finder()
                .is_none_or(|finder| matches!(finder.walk(), Walk::Done { .. }));
            if done {
                return;
            }
            assert!(std::time::Instant::now() < deadline, "the walk never ended");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    /// The column of the terminal's cursor after a draw.
    fn cursor_column(&mut self) -> u16 {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let app = &mut self.app;
        terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
        terminal.get_cursor_position().unwrap().x
    }
}

// ---- the palette

fn palette_query(rig: &Rig) -> String {
    rig.app.state().palette().unwrap().query().to_owned()
}

fn palette_titles(rig: &Rig) -> Vec<String> {
    rig.app
        .state()
        .palette()
        .unwrap()
        .visible()
        .map(|entry| entry.title.clone())
        .collect()
}

#[test]
fn a_letter_typed_in_the_palette_goes_in_at_the_cursor_and_the_matches_follow() {
    let mut rig = rig();
    rig.app.run_action(Action::CommandPalette);

    rig.type_text("sve");
    rig.key(KeyCode::Left);
    rig.key(KeyCode::Left);
    rig.type_text("a");

    assert_eq!(palette_query(&rig), "save");
    assert!(
        palette_titles(&rig)
            .iter()
            .any(|title| title.contains("Save")),
        "{:?}",
        palette_titles(&rig)
    );
}

#[test]
fn the_palette_cursor_moves_by_character_word_and_to_the_ends_and_the_terminal_cursor_follows() {
    let mut rig = rig();
    rig.app.run_action(Action::CommandPalette);
    rig.type_text("tab next");
    let end = rig.cursor_column();

    rig.key(KeyCode::Left);
    assert_eq!(rig.cursor_column(), end - 1);
    rig.press(KeyCode::Left, KeyModifiers::CONTROL);
    assert_eq!(rig.cursor_column(), end - 4, "to the start of `next`");
    rig.key(KeyCode::Home);
    assert_eq!(rig.cursor_column(), end - 8);
    rig.key(KeyCode::End);
    assert_eq!(rig.cursor_column(), end);
    rig.key(KeyCode::Home);
    rig.press(KeyCode::Right, KeyModifiers::CONTROL);
    assert_eq!(rig.cursor_column(), end - 5, "to the end of `tab`");
}

#[test]
fn delete_and_backspace_in_the_palette_edit_at_the_cursor() {
    let mut rig = rig();
    rig.app.run_action(Action::CommandPalette);
    rig.type_text("xsave");

    rig.key(KeyCode::Home);
    rig.key(KeyCode::Delete);
    assert_eq!(palette_query(&rig), "save");

    rig.key(KeyCode::End);
    rig.key(KeyCode::Left);
    rig.key(KeyCode::Backspace);
    assert_eq!(palette_query(&rig), "save"[..2].to_owned() + "e");
    rig.key(KeyCode::Delete);
    assert_eq!(palette_query(&rig), "sa");
}

#[test]
fn a_movement_in_the_palette_keeps_the_highlight_where_it_is() {
    let mut rig = rig();
    rig.app.run_action(Action::CommandPalette);
    rig.type_text("tab");
    rig.key(KeyCode::Down);
    let chosen = rig.app.state().palette().unwrap().selected();

    rig.key(KeyCode::Left);
    rig.key(KeyCode::Home);

    assert_eq!(rig.app.state().palette().unwrap().selected(), chosen);
}

// ---- the file finder

#[test]
fn the_finder_takes_a_letter_in_the_middle_and_ranks_again() {
    let mut rig = rig();
    rig.app.run_action(Action::FindFile);

    rig.finish_walk();
    rig.type_text("gam.rs");
    rig.key(KeyCode::Home);
    rig.key(KeyCode::Right);
    rig.key(KeyCode::Right);
    rig.key(KeyCode::Right);
    rig.type_text("m");

    assert_eq!(rig.app.state().finder().unwrap().query(), "gamm.rs");
    rig.key(KeyCode::Delete);
    assert_eq!(
        rig.app.state().finder().unwrap().query(),
        "gammrs",
        "the dot was at the cursor"
    );
    let target = rig.app.state().finder().unwrap().selected_target().unwrap();
    assert!(target.path.ends_with("gamma.rs"), "{:?}", target.path);
}

#[test]
fn the_finder_after_a_middle_edit_does_not_keep_files_the_new_text_no_longer_matches() {
    let mut rig = rig();
    rig.app.run_action(Action::FindFile);
    rig.finish_walk();
    rig.type_text("alpha");
    assert!(
        rig.app
            .state()
            .finder()
            .unwrap()
            .selected_target()
            .is_some()
    );

    // Growing the text at the start must look at every file again, not only the old candidates.
    rig.key(KeyCode::Home);
    rig.type_text("g");

    let target = rig.app.state().finder().unwrap().selected_target();
    assert!(target.is_none_or(|t| !t.path.ends_with("beta.txt")));
}

// ---- the project search

#[test]
fn the_project_search_edits_at_the_cursor_and_searches_the_new_text() {
    let mut rig = rig();
    rig.app.run_action(Action::FindInProject);

    rig.type_text("fnd me");
    rig.key(KeyCode::Home);
    rig.key(KeyCode::Right);
    rig.type_text("i");

    assert_eq!(rig.app.state().search().unwrap().query(), "find me");
    rig.key(KeyCode::End);
    rig.press(KeyCode::Left, KeyModifiers::CONTROL);
    rig.key(KeyCode::Delete);
    assert_eq!(rig.app.state().search().unwrap().query(), "find e");
}

// ---- the find bar

#[test]
fn the_find_bar_edits_the_query_at_the_cursor_and_finds_again() {
    let mut rig = rig();
    rig.app.run_action(Action::FindInFile);

    rig.type_text("fnd me");
    rig.key(KeyCode::Home);
    rig.key(KeyCode::Right);
    rig.type_text("i");

    let find = rig.app.state().find().unwrap();
    assert_eq!(find.query(), "find me");
    assert_eq!(find.matches().len(), 1, "the line `find me here`");
}

#[test]
fn the_find_bar_keeps_a_cursor_for_the_query_and_one_for_the_replacement() {
    let mut rig = rig();
    rig.app.run_action(Action::FindInFile);
    rig.type_text("ab");
    rig.key(KeyCode::Left);
    rig.press(KeyCode::Char('r'), KeyModifiers::CONTROL);
    rig.type_text("xz");
    rig.key(KeyCode::Left);
    rig.type_text("y");
    assert_eq!(rig.app.state().find().unwrap().replacement(), "xyz");

    rig.key(KeyCode::Tab);
    rig.type_text("c");

    let find = rig.app.state().find().unwrap();
    assert_eq!(
        find.query(),
        "acb",
        "the query's cursor stayed between a and b"
    );
    assert_eq!(find.replacement(), "xyz");
}

#[test]
fn arrows_in_the_find_bar_move_its_cursor_and_not_the_editors() {
    let mut rig = rig();
    rig.app.run_action(Action::FindInFile);
    rig.type_text("find");
    let before = *rig.app.state().editor().selections().primary();

    rig.key(KeyCode::Left);
    rig.key(KeyCode::Right);
    rig.key(KeyCode::Home);

    assert_eq!(*rig.app.state().editor().selections().primary(), before);
}

#[test]
fn a_long_query_scrolls_so_the_cursor_stays_on_the_screen() {
    let mut rig = rig();
    rig.app.run_action(Action::CommandPalette);
    rig.type_text(&"x".repeat(200));

    let column = rig.cursor_column();

    assert!(column < 100, "{column}");
    rig.key(KeyCode::Home);
    assert!(
        rig.cursor_column() < column,
        "back at the start of the text"
    );
}
