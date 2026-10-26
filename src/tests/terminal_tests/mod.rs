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
use crate::keymap::Keymap;
use crate::pty::Shell;
use crate::terminal::KeyboardSupport;
use crate::ui::ComponentKind;

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

/// A shell that has printed 100 numbers and then shows the bytes it gets.
const HISTORY_THEN_BYTES: &str = "seq 1 100; stty raw -echo; cat -v";

fn rig_with_history() -> (Rig, tempfile::TempDir) {
    let (mut rig, dir) = rig(HISTORY_THEN_BYTES);
    rig.toggle();
    rig.wait_for_text("100");
    std::thread::sleep(Duration::from_millis(200));
    (rig, dir)
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

impl Rig {
    /// Runs "Terminal: New Tab" from the palette, which works from the editor too.
    fn new_tab_from_the_palette(&mut self) {
        self.press(KeyCode::F(1), KeyModifiers::NONE);
        self.type_text("Terminal: New Tab");
        self.press(KeyCode::Enter, KeyModifiers::NONE);
        self.draw();
    }
}

mod basics;
mod links;
mod resize;
mod selection;
mod shell_cannot_start;
mod toggle_chord;
