use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::{AppState, Focus};
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

    /// The chord that toggles the pane: `ctrl+k t` from the editor, `ctrl+b t`
    /// from inside the terminal.
    fn toggle(&mut self) {
        let prefix = if self.app.state().focus() == Focus::Terminal {
            'b'
        } else {
            'k'
        };
        self.press(KeyCode::Char(prefix), KeyModifiers::CONTROL);
        self.press(KeyCode::Char('t'), KeyModifiers::NONE);
        self.draw();
    }

    fn press(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.press(KeyCode::Char(c), KeyModifiers::NONE);
        }
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

/// A shell that shows the bytes it receives, with the terminal in raw mode
/// the way a line editor leaves it, so keys arrive as they are.
const SHOW_BYTES: &str = "stty raw -echo; cat -v";

fn rig_showing_bytes() -> (Rig, tempfile::TempDir) {
    let (mut rig, dir) = rig(SHOW_BYTES);
    rig.toggle();
    // Give the shell a moment to switch modes before the first key.
    rig.wait_for("the shell to be running", |rig| {
        rig.app.state().terminal().is_running()
    });
    std::thread::sleep(Duration::from_millis(200));
    (rig, dir)
}

#[test]
fn showing_the_terminal_gives_it_the_keyboard() {
    let (mut rig, _dir) = rig("sleep 30");

    rig.toggle();

    assert_eq!(rig.app.state().focus(), Focus::Terminal);
}

#[test]
fn toggling_from_the_editor_with_the_pane_already_shown_moves_the_keyboard_into_it() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();
    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('e'), KeyModifiers::NONE);
    assert_eq!(rig.app.state().focus(), Focus::Editor);

    rig.toggle();

    assert_eq!(rig.app.state().focus(), Focus::Terminal);
    assert!(rig.screen().contains("Terminal"), "still shown");
}

#[test]
fn typed_keys_reach_the_shell() {
    let (mut rig, _dir) = rig("cat");
    rig.toggle();

    rig.type_text("hello there");
    rig.press(KeyCode::Enter, KeyModifiers::NONE);

    rig.wait_for("the echo", |rig| {
        rig.screen().matches("hello there").count() >= 2
    });
}

#[test]
fn special_keys_are_sent_as_the_bytes_a_terminal_sends() {
    let (mut rig, _dir) = rig_showing_bytes();

    rig.press(KeyCode::Up, KeyModifiers::NONE);
    rig.press(KeyCode::Char('c'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Enter, KeyModifiers::NONE);
    rig.press(KeyCode::Delete, KeyModifiers::NONE);

    rig.wait_for_text("^[[A^C^M^[[3~");
}

#[test]
fn keys_the_editor_binds_go_to_the_shell_while_the_terminal_has_the_keyboard() {
    let (mut rig, _dir) = rig_showing_bytes();

    rig.press(KeyCode::Char('q'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('p'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('s'), KeyModifiers::CONTROL);

    rig.wait_for_text("^Q^P^S");
    assert!(!rig.app.should_quit());
}

#[test]
fn ctrl_b_twice_sends_a_literal_ctrl_b() {
    let (mut rig, _dir) = rig_showing_bytes();

    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);

    rig.wait_for_text("^B");
}

#[test]
fn ctrl_b_alone_sends_nothing() {
    let (mut rig, _dir) = rig_showing_bytes();

    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    std::thread::sleep(Duration::from_millis(300));
    rig.wait_for("a redraw", |_| true);

    assert!(!rig.screen().contains("^B"), "{}", rig.screen());
}

#[test]
fn behind_the_prefix_e_goes_to_the_editor_and_x_to_the_explorer() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();

    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('x'), KeyModifiers::NONE);
    assert_eq!(rig.app.state().focus(), Focus::Explorer);

    rig.toggle();
    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('e'), KeyModifiers::NONE);
    assert_eq!(rig.app.state().focus(), Focus::Editor);
    assert!(rig.app.state().terminal().is_running(), "the shell goes on");
}

#[test]
fn behind_the_prefix_t_hides_the_pane_and_returns_the_keyboard_to_the_editor() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();

    rig.toggle();

    assert_eq!(rig.app.state().focus(), Focus::Editor);
    assert!(!rig.screen().contains("Terminal"), "{}", rig.screen());
}

#[test]
fn behind_the_prefix_a_binding_of_the_keymap_works() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();

    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('p'), KeyModifiers::CONTROL);

    assert!(rig.app.state().finder().is_some(), "the file finder opened");
}

#[test]
fn behind_the_prefix_a_binding_with_more_keys_goes_on_through_the_keymap() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();

    // `ctrl+k b` hides and shows the explorer; ctrl+k alone is half a binding.
    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('k'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('b'), KeyModifiers::NONE);
    rig.draw();

    assert!(!rig.screen().contains("Explorer"), "{}", rig.screen());
}

#[test]
fn an_unbound_key_behind_the_prefix_cancels_it_and_says_so() {
    let (mut rig, _dir) = rig_showing_bytes();

    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('z'), KeyModifiers::NONE);
    rig.type_text("ok");

    rig.wait_for_text("ok");
    assert!(
        rig.notifications().contains("ctrl+b z is not bound"),
        "{}",
        rig.notifications()
    );
    assert!(
        !rig.screen().contains("zok"),
        "the z did not reach the shell"
    );
}

#[test]
fn escape_behind_the_prefix_cancels_it_quietly() {
    let (mut rig, _dir) = rig_showing_bytes();

    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Esc, KeyModifiers::NONE);
    rig.type_text("ok");

    rig.wait_for_text("ok");
    assert_eq!(rig.notifications(), "");
    assert!(!rig.screen().contains("^["), "the escape was not sent");
}

#[test]
fn the_prefix_gives_up_silently_after_a_second() {
    let (mut rig, _dir) = rig_showing_bytes();
    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    let deadline = rig.app.pending_deadline().expect("the prefix is waiting");

    std::thread::sleep(
        deadline.saturating_duration_since(std::time::Instant::now()) + Duration::from_millis(20),
    );
    rig.app.expire_pending();
    rig.type_text("ok");

    rig.wait_for_text("ok");
    assert_eq!(rig.notifications(), "");
    assert_eq!(rig.app.pending_deadline(), None);
}

#[test]
fn the_status_bar_shows_that_the_prefix_is_waiting() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();

    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.draw();
    assert!(rig.screen().contains("ctrl+b ..."), "{}", rig.screen());

    rig.press(KeyCode::Char('e'), KeyModifiers::NONE);
    rig.draw();
    assert!(!rig.screen().contains("ctrl+b ..."), "{}", rig.screen());
}

#[test]
fn a_plain_paste_goes_to_the_shell_with_carriage_returns() {
    let (mut rig, _dir) = rig_showing_bytes();

    rig.app.handle_input(InputEvent::Paste("a\nb".to_owned()));

    rig.wait_for_text("a^Mb");
    assert_eq!(
        rig.app.state().editor().buffer().text(),
        "",
        "not into the editor"
    );
}

#[test]
fn a_program_that_asked_for_bracketed_paste_gets_one() {
    let (mut rig, _dir) = rig("printf '\\033[?2004h'; stty raw -echo; cat -v");
    rig.toggle();
    rig.wait_for("the mode", |rig| {
        rig.app
            .state()
            .terminal()
            .session()
            .is_some_and(|session| session.with_screen(|screen| screen.bracketed_paste()))
    });
    std::thread::sleep(Duration::from_millis(200));

    rig.app.handle_input(InputEvent::Paste("a\nb".to_owned()));

    rig.wait_for_text("^[[200~a");
    rig.wait_for_text("^[[201~");
}

#[test]
fn a_paste_into_a_shell_that_is_not_reading_reports_how_much_went_through() {
    let (mut rig, _dir) = rig("stty raw -echo; sleep 30");
    rig.toggle();
    rig.wait_for("the shell", |rig| rig.app.state().terminal().is_running());
    std::thread::sleep(Duration::from_millis(300));

    let start = Instant::now();
    rig.app
        .handle_input(InputEvent::Paste("p".repeat(8 * 1024 * 1024)));

    assert!(
        start.elapsed() < Duration::from_millis(500),
        "{:?}",
        start.elapsed()
    );
    assert!(
        rig.notifications().contains("not reading"),
        "{}",
        rig.notifications()
    );
}

#[test]
fn clicking_the_pane_gives_it_the_keyboard_and_clicking_the_editor_takes_it_back() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let (mut rig, _dir) = rig("printf hello; sleep 30");
    rig.toggle();
    rig.wait_for_text("hello");
    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('e'), KeyModifiers::NONE);
    let screen = rig.screen();
    let (row, line) = screen
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("hello"))
        .unwrap();
    let column = line.chars().position(|c| c == 'h').unwrap() as u16;
    let click = |column: u16, row: u16| {
        InputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    };

    rig.app.handle_input(click(column, row as u16));
    assert_eq!(rig.app.state().focus(), Focus::Terminal);

    rig.app.handle_input(click(60, 2));
    assert_eq!(rig.app.state().focus(), Focus::Editor);
}

#[test]
fn the_cursor_is_where_the_shells_is_while_the_pane_has_the_keyboard() {
    let (mut rig, _dir) = rig("printf abc; sleep 30");
    rig.toggle();
    rig.wait_for_text("abc");
    let screen = rig.screen();
    let (row, line) = screen
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("abc"))
        .unwrap();
    // The backend's text form wraps every line in a quote.
    let start = line.chars().position(|c| c == 'a').unwrap() as u16 - 1;

    let cursor = rig.terminal.get_cursor_position().unwrap();

    assert_eq!((cursor.x, cursor.y), (start + 3, row as u16));
}

#[test]
fn typing_after_the_shell_ended_says_so_instead_of_failing_silently() {
    let (mut rig, _dir) = rig("exit 0");
    rig.toggle();
    rig.wait_for("the shell to end", |rig| {
        !rig.app.state().terminal().is_running()
    });

    rig.type_text("x");

    assert!(
        rig.notifications().contains("has ended"),
        "{}",
        rig.notifications()
    );
}
