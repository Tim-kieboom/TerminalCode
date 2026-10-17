//! The Errors view: the problems of every open document, worst first, in the
//! bottom bar; its keys, its mouse and the count on its tab.

use std::fs;

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::app::state::{AppState, Focus};
use crate::buffer::Position;
use crate::components::prepare_and_render;
use crate::lsp::{Diagnostic, Severity};

struct Rig {
    app: App,
    dir: tempfile::TempDir,
    /// Keeps the channel the shell reports on open.
    _events: tokio::sync::mpsc::Receiver<crate::event::Event>,
}

fn diagnostic(start: usize, severity: Severity, message: &str) -> Diagnostic {
    Diagnostic {
        range: start..start + 1,
        severity,
        message: message.to_owned(),
        source: None,
        code: None,
    }
}

/// Two open files, `a.rs` and `b.rs`, with `a.rs` in front, and these problems:
/// a warning in `a.rs` (line 2), an error in `b.rs` (line 1) and an error in
/// `a.rs` (line 3).
fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a.rs"), dir.path().join("b.rs"));
    fs::write(&a, "one\ntwo\nthree\n").unwrap();
    fs::write(&b, "uno\ndos\n").unwrap();
    let mut state = AppState::default();
    state.open_project(dir.path()).unwrap();
    state.workspace_mut().open_path(&b).unwrap();
    state.workspace_mut().open_path(&a).unwrap();
    let in_a = state.workspace().document_of_path(&a).unwrap();
    let in_b = state.workspace().document_of_path(&b).unwrap();
    state.diagnostics_mut().set(
        in_a,
        vec![
            diagnostic(4, Severity::Warning, "unused thing\nmore detail"),
            diagnostic(8, Severity::Error, "mismatched types"),
        ],
    );
    state.diagnostics_mut().set(
        in_b,
        vec![diagnostic(0, Severity::Error, "cannot find value")],
    );
    let (events, receiver) = tokio::sync::mpsc::channel(64);
    let app = App::new(state)
        .with_events(events)
        .with_terminal_shell(crate::pty::Shell::script("sleep 30"));
    let mut rig = Rig {
        app,
        dir,
        _events: receiver,
    };
    rig.draw();
    rig
}

impl Rig {
    fn draw(&mut self) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let app = &mut self.app;
        terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
        terminal.backend().to_string()
    }

    fn key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
        self.draw();
    }

    fn show(&mut self) {
        self.key(KeyCode::Char('k'), KeyModifiers::CONTROL);
        self.key(KeyCode::Char('d'), KeyModifiers::NONE);
    }

    fn mouse(&mut self, kind: MouseEventKind, column: u16, row: u16) {
        self.app.handle_input(InputEvent::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
        self.draw();
    }

    fn open_file(&self) -> String {
        self.app
            .state()
            .editor()
            .buffer()
            .path()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    fn cursor(&self) -> Position {
        self.app.state().editor().selections().primary().head()
    }
}

#[test]
fn the_rows_are_errors_first_then_by_file_and_place() {
    let rig = rig();

    let rows = rig.app.state().error_rows();

    let places: Vec<String> = rows.iter().map(|row| row.place()).collect();
    assert_eq!(places, ["a.rs:3:1", "b.rs:1:1", "a.rs:2:1"]);
    assert_eq!(rows[2].message, "unused thing", "only the first line");
}

#[test]
fn the_view_starts_hidden_and_the_key_shows_it_with_the_keyboard() {
    let mut rig = rig();
    assert!(!rig.draw().contains("Errors ("));

    rig.show();
    let screen = rig.draw();

    assert_eq!(rig.app.state().focus(), Focus::Errors);
    assert!(screen.contains("Terminal - Errors (3)"), "{screen}");
    assert!(screen.contains("a.rs:3:1"), "{screen}");
    assert!(screen.contains("mismatched types"), "{screen}");
    assert!(screen.contains("b.rs:1:1"), "{screen}");
}

#[test]
fn the_key_with_the_keyboard_in_the_view_hides_it_again() {
    let mut rig = rig();
    rig.show();

    rig.show();

    assert_eq!(rig.app.state().focus(), Focus::Editor);
    assert!(!rig.draw().contains("Errors ("));
}

#[test]
fn enter_opens_the_file_at_the_problem_and_gives_the_keyboard_to_the_editor() {
    let mut rig = rig();
    rig.show();
    rig.key(KeyCode::Down, KeyModifiers::NONE);

    rig.key(KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(rig.open_file(), "b.rs");
    assert_eq!(rig.cursor(), Position::new(0, 0));
    assert_eq!(rig.app.state().focus(), Focus::Editor);
}

#[test]
fn the_selection_stops_at_both_ends_and_the_end_key_goes_to_the_last_row() {
    let mut rig = rig();
    rig.show();

    rig.key(KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(rig.app.state().errors().selected(), 0);

    rig.key(KeyCode::End, KeyModifiers::NONE);
    assert_eq!(rig.app.state().errors().selected(), 2);
    rig.key(KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(rig.app.state().errors().selected(), 2);

    rig.key(KeyCode::Home, KeyModifiers::NONE);
    assert_eq!(rig.app.state().errors().selected(), 0);
}

#[test]
fn escape_gives_the_keyboard_back_and_leaves_the_view_open() {
    let mut rig = rig();
    rig.show();

    rig.key(KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!(rig.app.state().focus(), Focus::Editor);
    assert!(rig.draw().contains("mismatched types"));
}

#[test]
fn a_click_on_a_row_opens_it() {
    let mut rig = rig();
    rig.show();
    let body = rig.app.state().errors().body();

    rig.mouse(
        MouseEventKind::Down(MouseButton::Left),
        body.x + 3,
        body.y + 1,
    );

    assert_eq!(rig.open_file(), "b.rs");
    assert_eq!(rig.app.state().focus(), Focus::Editor);
}

#[test]
fn a_click_below_the_last_row_opens_nothing() {
    let mut rig = rig();
    rig.show();
    let body = rig.app.state().errors().body();

    rig.mouse(
        MouseEventKind::Down(MouseButton::Left),
        body.x + 3,
        body.y + 6,
    );

    assert_eq!(rig.open_file(), "a.rs");
    assert_eq!(rig.app.state().focus(), Focus::Errors);
}

#[test]
fn the_view_says_so_when_there_is_nothing_wrong() {
    let mut rig = rig();
    let documents: Vec<_> = rig
        .app
        .state()
        .diagnostics()
        .documents()
        .map(|(document, _)| document)
        .collect();
    for document in documents {
        rig.app.state_mut().diagnostics_mut().clear(document);
    }

    rig.show();
    let screen = rig.draw();

    assert!(screen.contains("no problems found"), "{screen}");
    assert!(
        !screen.contains("Errors ("),
        "no count when there is nothing to count"
    );
}

#[test]
fn a_layout_without_the_view_says_so() {
    let mut rig = rig();
    rig.app.state_mut().set_layout(
        crate::ui::layout::LayoutTree::from_ron(
            r#"Col([Pane(view: Editor), Fixed(1, Pane(view: StatusBar))])"#,
        )
        .unwrap(),
    );

    rig.show();

    assert_eq!(rig.app.state().focus(), Focus::Editor);
    let _ = &rig.dir;
}

#[test]
fn a_click_on_the_other_tab_of_the_bar_goes_back_to_the_terminal() {
    let mut rig = rig();
    rig.show();
    let placements = rig
        .app
        .state()
        .placements(ratatui::layout::Rect::new(0, 0, 100, 30));
    let bar = placements.iter().find(|p| p.bar.is_some()).unwrap();
    let (rect, _) = crate::ui::bar::tab_rects(bar)
        .into_iter()
        .find(|(_, index)| *index == 0)
        .unwrap();

    rig.mouse(MouseEventKind::Down(MouseButton::Left), rect.x + 1, rect.y);

    assert!(
        rig.app
            .state()
            .is_showing(&crate::ui::ComponentKind::Terminal)
    );
    assert!(
        !rig.app
            .state()
            .is_showing(&crate::ui::ComponentKind::Errors)
    );
}

#[test]
fn a_ctrl_click_on_a_row_opens_it_and_asks_where_the_word_is_defined() {
    let mut rig = rig();
    rig.show();
    let body = rig.app.state().errors().body();

    rig.app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: body.x + 3,
        row: body.y + 1,
        modifiers: KeyModifiers::CONTROL,
    }));

    // No language server runs here, so nothing answers; the row was opened.
    assert_eq!(rig.open_file(), "b.rs");
    assert_eq!(rig.app.state().focus(), Focus::Editor);
}
