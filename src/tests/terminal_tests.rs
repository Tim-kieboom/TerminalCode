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
use crate::components::{ComponentKind, prepare_and_render};
use crate::event::Event;
use crate::keymap::Keymap;
use crate::pty::Shell;
use crate::terminal::KeyboardSupport;

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

    /// The chord that toggles the pane, `ctrl+``, from the editor or from inside
    /// the terminal.
    fn toggle(&mut self) {
        self.press(KeyCode::Char('`'), KeyModifiers::CONTROL);
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

// Prints the directory with `pwd`; Git's sh on Windows writes it as /c/Users/..., so Windows has its own test with cmd.exe (windows_tests.rs).
#[cfg(unix)]
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

    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char('`'),
        KeyModifiers::CONTROL,
    )));
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

// A Unix terminal in raw mode stops reading, which fills its input; ConPTY takes input into a console buffer that does not fill the same way, so this cannot be set up there.
#[cfg(unix)]
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

/// A shell that has printed 100 numbers and then shows the bytes it gets.
const HISTORY_THEN_BYTES: &str = "seq 1 100; stty raw -echo; cat -v";

fn rig_with_history() -> (Rig, tempfile::TempDir) {
    let (mut rig, dir) = rig(HISTORY_THEN_BYTES);
    rig.toggle();
    rig.wait_for_text("100");
    std::thread::sleep(Duration::from_millis(200));
    (rig, dir)
}

#[test]
fn shift_page_up_scrolls_into_the_history_and_says_how_far() {
    let (mut rig, _dir) = rig_with_history();
    assert!(!rig.screen().contains('↑'));

    rig.press(KeyCode::PageUp, KeyModifiers::SHIFT);
    rig.draw();

    let (rows, _) = rig.app.state().terminal().size().unwrap();
    assert!(
        rig.screen().contains(&format!("↑{}", rows - 1)),
        "{}",
        rig.screen()
    );
    assert!(!rig.screen().contains("100"), "the last line scrolled off");
}

#[test]
fn shift_page_down_comes_back_and_the_indicator_goes() {
    let (mut rig, _dir) = rig_with_history();
    rig.press(KeyCode::PageUp, KeyModifiers::SHIFT);

    rig.press(KeyCode::PageDown, KeyModifiers::SHIFT);
    rig.draw();

    assert!(!rig.screen().contains('↑'), "{}", rig.screen());
    assert!(rig.screen().contains("100"));
}

#[test]
fn shift_home_and_shift_end_jump_to_the_ends() {
    let (mut rig, _dir) = rig_with_history();

    rig.press(KeyCode::Home, KeyModifiers::SHIFT);
    rig.draw();
    assert!(rig.screen().contains('↑'), "{}", rig.screen());
    assert!(
        rig.screen().contains("\n") && rig.screen().contains(" 1 "),
        "{}",
        rig.screen()
    );

    rig.press(KeyCode::End, KeyModifiers::SHIFT);
    rig.draw();
    assert!(!rig.screen().contains('↑'));
}

#[test]
fn the_scroll_keys_are_not_sent_to_the_shell() {
    let (mut rig, _dir) = rig_with_history();

    rig.press(KeyCode::PageUp, KeyModifiers::SHIFT);
    rig.press(KeyCode::Char('c'), KeyModifiers::CONTROL);
    rig.wait_for_text("^C");

    assert!(!rig.screen().contains("^[[5;2~"));
}

#[test]
fn typing_brings_the_view_back_to_the_present() {
    let (mut rig, _dir) = rig_with_history();
    rig.press(KeyCode::Home, KeyModifiers::SHIFT);

    rig.press(KeyCode::Char('x'), KeyModifiers::CONTROL);

    rig.wait_for_text("^X");
    assert!(!rig.screen().contains('↑'), "{}", rig.screen());
}

#[test]
fn a_paste_brings_the_view_back_too() {
    let (mut rig, _dir) = rig_with_history();
    rig.press(KeyCode::Home, KeyModifiers::SHIFT);

    rig.app.handle_input(InputEvent::Paste("pasted".to_owned()));

    rig.wait_for_text("pasted");
    assert!(!rig.screen().contains('↑'));
}

#[test]
fn the_wheel_over_the_pane_scrolls_the_history_three_lines_a_notch() {
    use crossterm::event::{MouseEvent, MouseEventKind};
    let (mut rig, _dir) = rig_with_history();
    let screen = rig.screen();
    let (row, line) = screen
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("100"))
        .unwrap();
    let column = line.chars().position(|c| c == '1').unwrap() as u16;
    let wheel = |kind| {
        InputEvent::Mouse(MouseEvent {
            kind,
            column,
            row: row as u16,
            modifiers: KeyModifiers::NONE,
        })
    };

    rig.app.handle_input(wheel(MouseEventKind::ScrollUp));
    rig.app.handle_input(wheel(MouseEventKind::ScrollUp));
    rig.draw();
    assert!(rig.screen().contains("↑6"), "{}", rig.screen());

    rig.app.handle_input(wheel(MouseEventKind::ScrollDown));
    rig.draw();
    assert!(rig.screen().contains("↑3"), "{}", rig.screen());
}

#[test]
fn the_wheel_elsewhere_does_not_scroll_the_terminal() {
    use crossterm::event::{MouseEvent, MouseEventKind};
    let (mut rig, _dir) = rig_with_history();

    rig.app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 70,
        row: 2,
        modifiers: KeyModifiers::NONE,
    }));
    rig.draw();

    assert!(!rig.screen().contains('↑'), "{}", rig.screen());
}

#[test]
fn new_output_does_not_pull_a_scrolled_view_back() {
    let (mut rig, _dir) = rig("seq 1 60; sleep 1; seq 61 70; sleep 30");
    rig.toggle();
    rig.wait_for_text("60");
    rig.press(KeyCode::PageUp, KeyModifiers::SHIFT);
    let received = rig
        .app
        .state()
        .terminal()
        .session()
        .unwrap()
        .bytes_received();

    rig.wait_for("more output", |rig| {
        rig.app
            .state()
            .terminal()
            .session()
            .unwrap()
            .bytes_received()
            > received
    });
    rig.draw();

    assert!(rig.screen().contains('↑'), "{}", rig.screen());
}

#[test]
fn a_full_screen_program_gets_the_scroll_keys_and_has_no_indicator() {
    let (mut rig, _dir) = rig("printf '\\033[?1049h\\033[2Jalt'; stty raw -echo; cat -v");
    rig.toggle();
    rig.wait_for_text("alt");
    std::thread::sleep(Duration::from_millis(200));

    rig.press(KeyCode::PageUp, KeyModifiers::SHIFT);

    rig.wait_for_text("^[[5;2~");
    assert!(!rig.screen().contains('↑'));
}

// ---- the toggle chord closes the pane again

#[test]
fn the_chord_that_opened_the_terminal_closes_it_and_the_editor_has_the_keyboard() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();
    assert_eq!(rig.app.state().focus(), Focus::Terminal);

    rig.toggle();

    assert_eq!(rig.app.state().focus(), Focus::Editor);
    assert!(!rig.app.state().is_visible(&ComponentKind::Terminal));
    assert!(!rig.screen().contains("Terminal"), "{}", rig.screen());
}

#[test]
fn the_toggle_chord_does_not_reach_the_shell() {
    let (mut rig, _dir) = rig("cat");
    rig.toggle();
    rig.type_text("ab");
    rig.wait_for_text("ab");

    rig.toggle();
    rig.toggle();

    // Had the chord gone to the shell it would have been echoed or have changed
    // the line; the shell still shows exactly what was typed before.
    assert!(rig.screen().contains("ab"), "{}", rig.screen());
    assert_eq!(rig.app.state().focus(), Focus::Terminal);
}

#[test]
fn from_the_editor_the_chord_gives_a_shown_pane_the_keyboard_without_hiding_it() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();
    rig.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
    rig.press(KeyCode::Char('e'), KeyModifiers::NONE);
    assert_eq!(rig.app.state().focus(), Focus::Editor);

    rig.toggle();

    assert_eq!(rig.app.state().focus(), Focus::Terminal);
    assert!(rig.app.state().is_visible(&ComponentKind::Terminal));
}

#[test]
fn a_rebound_toggle_chord_closes_the_pane_too() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = AppState::default();
    state.open_project(dir.path()).unwrap();
    let mut keymap = Keymap::defaults(KeyboardSupport::Enhanced);
    keymap
        .apply_toml("[[binding]]\nkeys = \"f9\"\naction = \"toggle_terminal\"\n")
        .unwrap();
    let (tx, events) = mpsc::channel(1024);
    let app = App::with_keymap(state, keymap)
        .with_events(tx)
        .with_terminal_shell(Shell::script("sleep 30"));
    let mut rig = Rig {
        app,
        events,
        terminal: Terminal::new(TestBackend::new(100, 30)).unwrap(),
    };
    rig.draw();

    rig.press(KeyCode::F(9), KeyModifiers::NONE);
    rig.draw();
    assert_eq!(rig.app.state().focus(), Focus::Terminal);
    rig.press(KeyCode::F(9), KeyModifiers::NONE);

    assert_eq!(rig.app.state().focus(), Focus::Editor);
    assert!(!rig.app.state().is_visible(&ComponentKind::Terminal));
}

impl Rig {
    /// `ctrl+b` and then `key`, from inside the terminal.
    fn prefixed(&mut self, key: char) {
        self.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
        self.press(KeyCode::Char(key), KeyModifiers::NONE);
        self.draw();
    }

    fn terminal_tabs(&self) -> usize {
        self.app.state().terminal().tab_count()
    }

    fn active_tab(&self) -> usize {
        self.app.state().terminal().active_tab()
    }

    fn pid_of(&self, tab: usize) -> u32 {
        self.app
            .state()
            .terminal()
            .session_of(tab)
            .unwrap()
            .process_id()
            .unwrap()
    }

    /// The screen cell of the first column and row of the shell's text.
    fn text_origin(&self) -> (u16, u16) {
        let terminal = self.app.state().terminal();
        for row in 0..30 {
            for column in 0..100 {
                if terminal.screen_cell(column, row) == Some((0, 0)) {
                    return (column, row);
                }
            }
        }
        panic!("the terminal has no text area");
    }
}

/// A shell that has a different first line in every tab.
const UNIQUE: &str = "echo tab-$$; sleep 30";

#[test]
fn ctrl_b_c_opens_a_second_tab_with_its_own_shell_and_a_tab_bar() {
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();
    assert!(
        rig.screen().contains(" 1: sh "),
        "the tab bar shows with one tab"
    );

    rig.prefixed('c');

    assert_eq!(rig.terminal_tabs(), 2);
    assert_eq!(rig.active_tab(), 1);
    assert_ne!(rig.pid_of(0), rig.pid_of(1));
    assert!(rig.screen().contains(" 1: sh "), "{}", rig.screen());
    assert!(rig.screen().contains(" 2: sh "), "{}", rig.screen());
}

#[test]
fn every_tab_has_the_size_of_the_text_area_with_the_tab_bar_taking_a_row() {
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();
    let (rows_alone, columns) = rig.app.state().terminal().size().unwrap();

    rig.prefixed('c');
    rig.draw();

    let (rows, _) = rig.app.state().terminal().size().unwrap();
    assert_eq!(
        rows, rows_alone,
        "the bar was there already, so nothing resizes"
    );
    for tab in 0..2 {
        let size = rig
            .app
            .state()
            .terminal()
            .session_of(tab)
            .unwrap()
            .with_screen(|screen| screen.size());
        assert_eq!(size, (rows, columns), "tab {}", tab + 1);
    }
}

#[test]
fn each_tab_keeps_its_own_screen() {
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();
    rig.wait_for_text("tab-");
    rig.prefixed('c');
    rig.wait_for("the second shell", |rig| {
        rig.app
            .state()
            .terminal()
            .session_of(1)
            .unwrap()
            .with_screen(|screen| screen.contents().contains("tab-"))
    });
    let text_of = |rig: &Rig, tab: usize| {
        rig.app
            .state()
            .terminal()
            .session_of(tab)
            .unwrap()
            .with_screen(|screen| screen.contents())
    };

    assert_ne!(
        text_of(&rig, 0),
        text_of(&rig, 1),
        "different shells, different pids"
    );
    rig.prefixed('1');
    rig.draw();
    let shown = rig.screen();
    assert!(shown.contains(text_of(&rig, 0).trim()), "{shown}");
}

#[test]
fn ctrl_b_n_and_p_go_between_tabs_and_digits_pick_one() {
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();
    rig.prefixed('c');
    rig.prefixed('c');
    assert_eq!(rig.active_tab(), 2);

    rig.prefixed('n');
    assert_eq!(rig.active_tab(), 0);
    rig.prefixed('p');
    assert_eq!(rig.active_tab(), 2);
    rig.prefixed('2');
    assert_eq!(rig.active_tab(), 1);
    rig.prefixed('9');
    assert_eq!(rig.active_tab(), 1, "there is no ninth tab");
}

#[test]
fn ctrl_b_w_closes_the_tab_and_stops_its_shell() {
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();
    rig.prefixed('c');
    let second = rig.pid_of(1);

    rig.prefixed('w');

    assert_eq!(rig.terminal_tabs(), 1);
    assert_eq!(rig.active_tab(), 0);
    assert_ne!(rig.pid_of(0), second);
    assert!(
        rig.screen().contains(" 1: sh "),
        "the bar stays with one tab"
    );
    assert!(!rig.screen().contains(" 2: sh "));
}

#[test]
fn closing_the_last_tab_hides_the_pane_and_returns_the_keyboard() {
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();

    rig.prefixed('w');

    assert_eq!(rig.terminal_tabs(), 0);
    assert_eq!(rig.app.state().focus(), Focus::Editor);
    assert!(!rig.screen().contains("Terminal"), "{}", rig.screen());
}

#[test]
fn a_tab_that_ended_is_marked_and_stays_until_it_is_closed() {
    let (mut rig, _dir) = rig("exit 0");
    rig.toggle();
    rig.wait_for("the shell to end", |rig| {
        !rig.app.state().terminal().is_running()
    });
    rig.prefixed('c');

    rig.wait_for("two ended tabs", |rig| {
        rig.screen().matches("(ended)").count() == 2
    });
    assert_eq!(rig.terminal_tabs(), 2);
}

#[test]
fn showing_the_pane_again_after_every_shell_ended_starts_over_with_one_tab() {
    let (mut rig, _dir) = rig("exit 0");
    rig.toggle();
    rig.wait_for("the shell to end", |rig| {
        !rig.app.state().terminal().is_running()
    });
    rig.prefixed('c');
    rig.wait_for("both to end", |rig| {
        !rig.app.state().terminal().any_running()
    });

    rig.toggle();
    rig.toggle();

    assert_eq!(rig.terminal_tabs(), 1);
}

#[test]
fn at_most_sixteen_tabs() {
    let (mut rig, _dir) = rig("sleep 30");
    rig.toggle();

    for _ in 0..20 {
        rig.prefixed('c');
    }

    assert_eq!(rig.terminal_tabs(), 16);
    assert!(
        rig.notifications().contains("at most 16"),
        "{}",
        rig.notifications()
    );
}

#[test]
fn clicking_a_tab_picks_it_and_a_middle_click_closes_it() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();
    rig.prefixed('c');
    let screen = rig.screen();
    let (row, line) = screen
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains(" 1: sh "))
        .unwrap();
    let first = line.find(" 1: sh ").unwrap() as u16;
    let click = |kind, column: u16| {
        InputEvent::Mouse(MouseEvent {
            kind,
            column: column - 1, // the text form of the backend starts with a quote
            row: row as u16,
            modifiers: KeyModifiers::NONE,
        })
    };

    rig.app
        .handle_input(click(MouseEventKind::Down(MouseButton::Left), first + 2));
    assert_eq!(rig.active_tab(), 0);

    rig.app
        .handle_input(click(MouseEventKind::Down(MouseButton::Middle), first + 2));
    assert_eq!(rig.terminal_tabs(), 1);
}

#[test]
fn the_palette_has_the_tab_commands() {
    use crate::event::action::Action;

    for (action, title) in [
        (Action::TerminalNewTab, "Terminal: New Tab"),
        (Action::TerminalNextTab, "Terminal: Next Tab"),
        (Action::TerminalPreviousTab, "Terminal: Previous Tab"),
        (Action::TerminalCloseTab, "Terminal: Close Tab"),
    ] {
        assert!(Action::palette_actions().contains(&action));
        assert_eq!(action.title(), Some(title));
    }
}

/// A shell that has turned on mouse reporting (button events and SGR
/// encoding) and shows what it receives.
const MOUSE_ON_SHOW_BYTES: &str = "printf '\\033[?1000h\\033[?1006h'; stty raw -echo; cat -v";

fn rig_with_mouse_on() -> (Rig, tempfile::TempDir) {
    let (mut rig, dir) = rig(MOUSE_ON_SHOW_BYTES);
    rig.toggle();
    rig.wait_for("mouse reporting", |rig| {
        rig.app.state().terminal().session().is_some_and(|session| {
            session.with_screen(|screen| {
                screen.mouse_protocol_mode() != vt100::MouseProtocolMode::None
            })
        })
    });
    std::thread::sleep(Duration::from_millis(200));
    (rig, dir)
}

fn mouse(
    kind: crossterm::event::MouseEventKind,
    column: u16,
    row: u16,
    modifiers: KeyModifiers,
) -> InputEvent {
    InputEvent::Mouse(crossterm::event::MouseEvent {
        kind,
        column,
        row,
        modifiers,
    })
}

#[test]
fn a_program_that_asked_for_the_mouse_gets_clicks_with_pane_relative_positions() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let (mut rig, _dir) = rig_with_mouse_on();
    let (x, y) = rig.text_origin();

    rig.app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Left),
        x + 4,
        y + 2,
        KeyModifiers::NONE,
    ));
    rig.app.handle_input(mouse(
        MouseEventKind::Up(MouseButton::Left),
        x + 4,
        y + 2,
        KeyModifiers::NONE,
    ));

    rig.wait_for_text("^[[<0;5;3M^[[<0;5;3m");
}

#[test]
fn the_wheel_goes_to_such_a_program_instead_of_scrolling_the_history() {
    use crossterm::event::MouseEventKind;
    let (mut rig, _dir) = rig_with_mouse_on();
    let (x, y) = rig.text_origin();

    rig.app
        .handle_input(mouse(MouseEventKind::ScrollUp, x, y, KeyModifiers::NONE));

    rig.wait_for_text("^[[<64;1;1M");
    assert!(!rig.screen().contains('↑'));
}

#[test]
fn shift_keeps_the_mouse_for_the_editor() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let (mut rig, _dir) = rig_with_mouse_on();
    let (x, y) = rig.text_origin();

    rig.app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Left),
        x + 1,
        y + 1,
        KeyModifiers::SHIFT,
    ));
    rig.type_text("ok");

    rig.wait_for_text("ok");
    assert!(!rig.screen().contains("^[[<"), "{}", rig.screen());
}

#[test]
fn a_program_that_did_not_ask_gets_no_mouse_reports() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let (mut rig, _dir) = rig_showing_bytes();
    let (x, y) = rig.text_origin();

    rig.app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Left),
        x + 1,
        y + 1,
        KeyModifiers::NONE,
    ));
    rig.type_text("ok");

    rig.wait_for_text("ok");
    assert!(!rig.screen().contains("^[[<"), "{}", rig.screen());
    assert_eq!(rig.app.state().focus(), Focus::Terminal);
}

#[test]
fn a_drag_inside_the_pane_does_not_select_text_in_the_editor() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let (mut rig, _dir) = rig_showing_bytes();
    let (x, y) = rig.text_origin();
    rig.app
        .state_mut()
        .workspace_mut()
        .with_editor(|editor| editor.insert_text("hello").unwrap());
    let before = *rig.app.state().editor().selections().primary();

    rig.app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Left),
        x + 3,
        y,
        KeyModifiers::NONE,
    ));
    rig.app.handle_input(mouse(
        MouseEventKind::Drag(MouseButton::Left),
        x + 9,
        y + 2,
        KeyModifiers::NONE,
    ));
    rig.app.handle_input(mouse(
        MouseEventKind::Up(MouseButton::Left),
        x + 9,
        y + 2,
        KeyModifiers::NONE,
    ));

    assert_eq!(rig.app.state().editor().selections().primary(), &before);
}

/// The screen cell of the "+" button, found by looking for it in the tab bar.
fn plus_button(rig: &Rig) -> (u16, u16) {
    let screen = rig.screen();
    let (row, line) = screen
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains(" 1: sh ") && line.contains(" + "))
        .unwrap_or_else(|| panic!("no + button in the tab bar:\n{screen}"));
    // Columns count characters, and the border characters are several bytes.
    let byte = line.find(" + ").unwrap();
    let column = line[..byte].chars().count() as u16;
    (column, row as u16)
}

#[test]
fn the_plus_button_sits_right_after_the_last_tab_even_with_a_single_tab() {
    let (mut rig, _dir) = rig("sleep 30");
    assert!(!rig.screen().contains(" + "), "hidden pane, no button");

    rig.toggle();

    plus_button(&rig);
    assert!(rig.screen().contains(" 1: sh  + "), "{}", rig.screen());
    rig.prefixed('c');
    assert!(rig.screen().contains(" 2: sh  + "), "{}", rig.screen());
}

#[test]
fn clicking_the_plus_button_opens_a_new_tab_and_gives_it_the_keyboard() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();
    rig.prefixed('e');
    let (x, y) = plus_button(&rig);

    rig.app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Left),
        x,
        y,
        KeyModifiers::NONE,
    ));

    assert_eq!(rig.terminal_tabs(), 2);
    assert_eq!(rig.active_tab(), 1);
    assert_eq!(rig.app.state().focus(), Focus::Terminal);
}

#[test]
fn the_plus_button_keeps_working_with_several_tabs() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();
    rig.prefixed('c');
    rig.draw();
    let (x, y) = plus_button(&rig);

    rig.app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Left),
        x + 1,
        y,
        KeyModifiers::NONE,
    ));

    assert_eq!(rig.terminal_tabs(), 3);
}

#[test]
fn only_a_click_on_the_plus_button_opens_a_tab() {
    use crossterm::event::{MouseButton, MouseEventKind};
    let (mut rig, _dir) = rig(UNIQUE);
    rig.toggle();
    let (x, y) = plus_button(&rig);

    rig.app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Right),
        x,
        y,
        KeyModifiers::NONE,
    ));
    rig.app.handle_input(mouse(
        MouseEventKind::Down(MouseButton::Left),
        x - 4,
        y,
        KeyModifiers::NONE,
    ));

    assert_eq!(rig.terminal_tabs(), 1);
}

// ---- a shell that cannot start

const MISSING_SHELL: &str = "/nonexistent/shell";

fn rig_with_shells(shell: Shell, fallback: Shell) -> (Rig, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut state = AppState::default();
    state.open_project(dir.path()).unwrap();
    let (tx, events) = mpsc::channel(1024);
    let app = App::new(state)
        .with_events(tx)
        .with_terminal_shell(shell)
        .with_fallback_shell(fallback);
    let mut rig = Rig {
        app,
        events,
        terminal: Terminal::new(TestBackend::new(100, 30)).unwrap(),
    };
    rig.draw();
    (rig, dir)
}

fn pane_shown(rig: &Rig) -> bool {
    rig.app.state().is_visible(&ComponentKind::Terminal)
}

#[test]
fn a_shell_that_cannot_start_leaves_no_empty_pane_and_the_keyboard_in_the_editor() {
    let (mut rig, _dir) =
        rig_with_shells(Shell::program(MISSING_SHELL), Shell::program(MISSING_SHELL));

    rig.toggle();

    assert!(!pane_shown(&rig), "the pane stayed open with no tab");
    assert_eq!(rig.app.state().focus(), Focus::Editor);
    assert_eq!(rig.app.state().terminal().tab_count(), 0);
    assert!(rig.app.state().notifications().has_errors());
    assert!(
        rig.notifications().contains(MISSING_SHELL),
        "{}",
        rig.notifications()
    );
}

#[test]
fn typing_after_a_failed_start_goes_to_the_editor() {
    let (mut rig, _dir) =
        rig_with_shells(Shell::program(MISSING_SHELL), Shell::program(MISSING_SHELL));
    rig.toggle();

    rig.type_text("hi");

    assert_eq!(rig.app.state().editor().buffer().text(), "hi");
}

#[test]
fn a_new_terminal_tab_that_cannot_start_leaves_the_pane_and_focus_as_they_were() {
    let (mut rig, _dir) =
        rig_with_shells(Shell::program(MISSING_SHELL), Shell::program(MISSING_SHELL));

    rig.new_tab_from_the_palette();

    assert!(!pane_shown(&rig));
    assert_eq!(rig.app.state().focus(), Focus::Editor);
}

// Unix only: the working shell is `sh -c`, which is not on every Windows machine.
#[cfg(unix)]
#[test]
fn a_shell_that_cannot_start_falls_back_to_the_platform_shell_and_the_notice_stays() {
    let (mut rig, _dir) = rig_with_shells(Shell::program(MISSING_SHELL), Shell::script("sleep 30"));

    rig.toggle();

    assert!(pane_shown(&rig));
    assert_eq!(rig.app.state().focus(), Focus::Terminal);
    assert_eq!(rig.app.state().terminal().tab_count(), 1);
    assert!(
        rig.app.state().notifications().has_errors(),
        "the notice must be sticky"
    );
    let notice = rig.notifications();
    assert!(
        notice.contains(MISSING_SHELL) && notice.contains("started `sh` instead"),
        "{notice}"
    );
}

// Unix only: the working shell is `sh -c`, which is not on every Windows machine.
#[cfg(unix)]
#[test]
fn after_a_fallback_the_next_tab_uses_it_without_another_notice() {
    let (mut rig, _dir) = rig_with_shells(Shell::program(MISSING_SHELL), Shell::script("sleep 30"));
    rig.toggle();
    assert!(
        rig.app.state().notifications().has_errors(),
        "the notice stays until a key is pressed"
    );
    rig.prefixed('c');

    assert_eq!(rig.app.state().terminal().tab_count(), 2);
    assert!(
        !rig.app.state().notifications().has_errors(),
        "the notice was dismissed by the key and not made again: {}",
        rig.notifications()
    );
}

// Unix only: the working shell is `sh -c`, which is not on every Windows machine.
#[cfg(unix)]
#[test]
fn a_working_shell_starts_with_no_notice() {
    let (mut rig, _dir) = rig_with_shells(Shell::script("sleep 30"), Shell::program(MISSING_SHELL));

    rig.toggle();

    assert_eq!(rig.app.state().terminal().tab_count(), 1);
    assert!(rig.app.state().notifications().len() == 0);
}

#[test]
fn a_shell_that_is_already_the_platform_shell_is_not_tried_twice() {
    let (mut rig, _dir) =
        rig_with_shells(Shell::program(MISSING_SHELL), Shell::program(MISSING_SHELL));

    rig.toggle();

    assert_eq!(rig.app.state().notifications().len(), 1);
}

impl Rig {
    /// Runs "Terminal: New Tab" from the palette, which works from the editor too.
    fn new_tab_from_the_palette(&mut self) {
        self.press(KeyCode::F(1), KeyModifiers::NONE);
        self.type_text("Terminal: New Tab");
        self.press(KeyCode::Enter, KeyModifiers::NONE);
        self.draw();
    }
}
