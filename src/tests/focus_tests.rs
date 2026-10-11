//! Moving the keyboard between the editor panes, the explorer and the
//! terminal with `alt+h/j/k/l`.

use std::fs;

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::{AppState, Focus};
use crate::components::{ComponentKind, prepare_and_render};
use crate::pty::Shell;

struct Screen {
    app: App,
    terminal: Terminal<TestBackend>,
    _dir: tempfile::TempDir,
}

fn screen() -> Screen {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "alpha\n").unwrap();
    let mut state = AppState::default();
    state.open_project(dir.path()).unwrap();
    let (tx, _events) = mpsc::channel(64);
    let app = App::new(state)
        .with_events(tx)
        .with_terminal_shell(Shell::script("sleep 30"));
    let mut screen = Screen {
        app,
        terminal: Terminal::new(TestBackend::new(100, 30)).unwrap(),
        _dir: dir,
    };
    screen.draw();
    screen
}

impl Screen {
    fn draw(&mut self) {
        let app = &mut self.app;
        self.terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
    }

    fn alt(&mut self, c: char) {
        self.app.handle_input(InputEvent::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::ALT,
        )));
        self.draw();
    }

    fn ctrl(&mut self, c: char) {
        self.app.handle_input(InputEvent::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::CONTROL,
        )));
        self.draw();
    }

    fn focus(&self) -> Focus {
        self.app.state().focus()
    }

    /// The terminal pane open with a shell in it, the keyboard back in the editor.
    fn with_terminal(mut self) -> Self {
        self.ctrl('`');
        assert_eq!(self.focus(), Focus::Terminal);
        // From inside the terminal: the prefix, then the chord.
        self.ctrl('b');
        self.alt('k');
        assert_eq!(self.focus(), Focus::Editor);
        self
    }
}

#[test]
fn left_from_the_leftmost_pane_goes_to_the_explorer() {
    let mut screen = screen();
    assert_eq!(screen.focus(), Focus::Editor);

    screen.alt('h');

    assert_eq!(screen.focus(), Focus::Explorer);
}

#[test]
fn right_from_the_explorer_goes_back_to_the_editor() {
    let mut screen = screen();
    screen.alt('h');

    screen.alt('l');

    assert_eq!(screen.focus(), Focus::Editor);
}

#[test]
fn up_and_down_from_the_explorer_stay_in_it() {
    let mut screen = screen();
    screen.alt('h');

    screen.alt('k');
    screen.alt('j');
    screen.alt('h');

    assert_eq!(screen.focus(), Focus::Explorer);
}

#[test]
fn right_from_the_editor_with_nothing_there_stays() {
    let mut screen = screen();

    screen.alt('l');

    assert_eq!(screen.focus(), Focus::Editor);
}

#[test]
fn a_hidden_explorer_is_not_a_target() {
    let mut screen = screen();
    screen.app.state_mut().hide_explorer();
    screen.draw();

    screen.alt('h');

    assert_eq!(screen.focus(), Focus::Editor);
}

#[test]
fn left_from_a_split_pane_goes_to_the_pane_not_the_explorer() {
    let mut screen = screen();
    screen
        .app
        .state_mut()
        .workspace_mut()
        .split(crate::ui::layout::Axis::Horizontal);
    screen.draw();
    let right = screen.app.state().workspace().focused_area();

    screen.alt('h');
    assert_eq!(screen.focus(), Focus::Editor);
    assert_ne!(screen.app.state().workspace().focused_area(), right);

    screen.alt('h');
    assert_eq!(screen.focus(), Focus::Explorer);
}

#[test]
fn down_from_the_editor_goes_to_the_terminal_and_up_comes_back() {
    let mut screen = screen().with_terminal();

    screen.alt('j');
    assert_eq!(screen.focus(), Focus::Terminal);

    // The terminal keeps every key for the shell; its prefix leads to the chord.
    screen.ctrl('b');
    screen.alt('k');
    assert_eq!(screen.focus(), Focus::Editor);
}

#[test]
fn left_from_the_terminal_goes_to_the_explorer() {
    let mut screen = screen().with_terminal();
    screen.alt('j');

    screen.ctrl('b');
    screen.alt('h');

    assert_eq!(screen.focus(), Focus::Explorer);
}

#[test]
fn down_from_the_explorer_does_not_reach_the_terminal_beside_the_editor() {
    // The terminal sits under the editor only; nothing is below the explorer.
    let mut screen = screen().with_terminal();
    screen.alt('h');
    assert_eq!(screen.focus(), Focus::Explorer);

    screen.alt('j');

    assert_eq!(screen.focus(), Focus::Explorer);
}

#[test]
fn a_hidden_terminal_is_not_a_target() {
    let mut screen = screen().with_terminal();
    screen.app.state_mut().hide_terminal();
    screen.draw();

    screen.alt('j');

    assert_eq!(screen.focus(), Focus::Editor);
    assert!(!screen.app.state().is_visible(&ComponentKind::Terminal));
}
