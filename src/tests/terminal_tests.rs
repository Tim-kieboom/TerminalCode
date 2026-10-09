use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::AppState;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::pty::Shell;

/// An app wired to an event channel, as in the real app, with a script as the
/// terminal's shell.
struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    terminal: Terminal<TestBackend>,
}

fn rig_in(project: &Path, script: &str) -> Rig {
    let mut state = AppState::default();
    state.open_project(project).unwrap();
    let (tx, events) = mpsc::channel(1024);
    let app = App::new(state)
        .with_events(tx)
        .with_terminal_shell(Shell::script(script));
    let mut rig = Rig {
        app,
        events,
        terminal: Terminal::new(TestBackend::new(100, 30)).unwrap(),
    };
    rig.draw();
    rig
}

fn rig(script: &str) -> (Rig, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "alpha\n").unwrap();
    (rig_in(dir.path(), script), dir)
}

impl Rig {
    fn draw(&mut self) {
        let app = &mut self.app;
        self.terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
    }

    /// `ctrl+k t`.
    fn toggle(&mut self) {
        for (code, modifiers) in [
            (KeyCode::Char('k'), KeyModifiers::CONTROL),
            (KeyCode::Char('t'), KeyModifiers::NONE),
        ] {
            self.app
                .handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
        }
        self.draw();
    }

    /// Takes what the shell reported until `condition` holds of the screen.
    fn wait_for(&mut self, what: &str, condition: impl Fn(&Rig) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            while let Ok(event) = self.events.try_recv() {
                self.app.handle_event(event);
            }
            self.draw();
            if condition(self) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}:\n{}",
                self.screen()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn wait_for_text(&mut self, text: &str) {
        self.wait_for(text, |rig| rig.screen().contains(text));
    }

    fn screen(&self) -> String {
        self.terminal.backend().to_string()
    }

    fn notifications(&self) -> String {
        self.app
            .state()
            .latest_notification()
            .unwrap_or_default()
            .to_owned()
    }
}

#[test]
fn the_terminal_is_hidden_until_it_is_asked_for() {
    let (mut rig, _dir) = rig("printf hello; sleep 30");

    assert!(!rig.screen().contains("Terminal"), "{}", rig.screen());
    assert!(!rig.app.state().terminal().is_running());

    rig.toggle();

    assert!(rig.screen().contains("Terminal"), "{}", rig.screen());
}

#[test]
fn showing_it_the_first_time_starts_a_shell_whose_output_is_drawn() {
    let (mut rig, _dir) = rig("printf hello; sleep 30");

    rig.toggle();

    rig.wait_for_text("hello");
    assert!(rig.app.state().terminal().is_running());
}

#[test]
fn toggling_again_hides_the_pane_and_keeps_the_same_shell() {
    let (mut rig, _dir) = rig("printf hello; sleep 30");
    rig.toggle();
    rig.wait_for_text("hello");
    let pid = rig.app.state().terminal().session().unwrap().process_id();

    rig.toggle();
    assert!(!rig.screen().contains("hello"), "{}", rig.screen());
    rig.toggle();

    assert!(rig.screen().contains("hello"));
    assert_eq!(
        rig.app.state().terminal().session().unwrap().process_id(),
        pid
    );
}

#[test]
fn the_shell_starts_in_the_project_root() {
    let (mut rig, dir) = rig("pwd; sleep 30");

    rig.toggle();

    let root = dir.path().canonicalize().unwrap();
    rig.wait_for_text(root.to_str().unwrap());
}

#[test]
fn colors_reach_the_screen() {
    let (mut rig, _dir) = rig("printf '\\033[31mred\\033[0m'; sleep 30");
    rig.toggle();
    rig.wait_for_text("red");

    let buffer = rig.terminal.backend().buffer().clone();
    let cell = buffer
        .content
        .iter()
        .find(|cell| cell.symbol() == "r" && cell.fg == Color::Indexed(1));

    assert!(cell.is_some(), "no red `r` on screen:\n{}", rig.screen());
}

#[test]
fn the_shell_has_the_size_of_the_pane() {
    let (mut rig, _dir) = rig("stty size; sleep 30");

    rig.toggle();

    let (rows, columns) = rig.app.state().terminal().size().unwrap();
    assert!(rows > 3 && columns > 20, "{rows}x{columns}");
    rig.wait_for_text(&format!("{rows} {columns}"));
}

#[test]
fn resizing_the_window_resizes_the_shell() {
    let (mut rig, _dir) = rig("trap 'stty size' WINCH; stty size; while true; do sleep 0.1; done");
    rig.toggle();
    let (rows, columns) = rig.app.state().terminal().size().unwrap();
    rig.wait_for_text(&format!("{rows} {columns}"));

    rig.terminal.backend_mut().resize(80, 30);
    rig.terminal
        .resize(ratatui::layout::Rect::new(0, 0, 80, 30))
        .unwrap();
    rig.draw();

    let (_, narrower) = rig.app.state().terminal().size().unwrap();
    assert!(narrower < columns);
    rig.wait_for_text(&format!("{rows} {narrower}"));
}

#[test]
fn a_hidden_pane_keeps_the_size_its_shell_last_had() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();
    let size = rig.app.state().terminal().size();

    rig.toggle();
    rig.draw();

    assert_eq!(rig.app.state().terminal().size(), size);
}

#[test]
fn a_shell_that_ends_is_reported_once_and_toggling_starts_another() {
    let (mut rig, _dir) = rig("exit 3");
    rig.toggle();

    rig.wait_for("the notification", |rig| {
        rig.notifications().contains("code 3")
    });
    assert!(!rig.app.state().terminal().is_running());

    // Hide, show: a new shell, which ends again.
    rig.toggle();
    rig.toggle();
    rig.wait_for("a second notification", |rig| {
        rig.notifications().contains("code 3") && rig.app.state().terminal().session().is_some()
    });
}

#[test]
fn without_an_event_loop_there_is_no_shell_and_a_message_says_so() {
    let mut app = App::new(AppState::default());
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();

    for (code, modifiers) in [
        (KeyCode::Char('k'), KeyModifiers::CONTROL),
        (KeyCode::Char('t'), KeyModifiers::NONE),
    ] {
        app.handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
    }
    terminal
        .draw(|frame| prepare_and_render(frame, app.state_mut()))
        .unwrap();

    assert!(!app.state().terminal().is_running());
    assert!(
        app.state()
            .latest_notification()
            .unwrap()
            .contains("event loop")
    );
}

#[test]
fn the_palette_offers_to_toggle_the_terminal() {
    use crate::event::action::Action;

    assert!(Action::palette_actions().contains(&Action::ToggleTerminal));
    assert_eq!(
        Action::ToggleTerminal.title(),
        Some("View: Toggle Terminal")
    );
}

#[test]
fn the_editor_gets_the_space_back_while_the_terminal_is_hidden() {
    let (mut rig, _dir) = rig("sleep 30");
    let tall = rig
        .app
        .state()
        .workspace()
        .active_editor()
        .view()
        .viewport_height();

    rig.toggle();
    let short = rig
        .app
        .state()
        .workspace()
        .active_editor()
        .view()
        .viewport_height();
    rig.toggle();
    let tall_again = rig
        .app
        .state()
        .workspace()
        .active_editor()
        .view()
        .viewport_height();

    assert!(short < tall, "{short} vs {tall}");
    assert_eq!(tall, tall_again);
}
