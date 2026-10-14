//! Where the cursor is shown: whoever gets the next keypress has the live
//! cursor, every other place that has one marks it with a dimmed block.

use std::fs;
use std::time::{Duration, Instant};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::app::App;
use crate::app::state::{AppState, Focus};
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::pty::Shell;
use crate::ui::layout::Axis;

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    terminal: Terminal<TestBackend>,
    _dir: tempfile::TempDir,
}

fn rig(script: &str) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.txt");
    fs::write(&file, "alpha\nbeta\n").unwrap();
    let mut state = AppState::default();
    state.open_project(dir.path()).unwrap();
    state.workspace_mut().open_path(&file).unwrap();
    let (tx, events) = mpsc::channel(1024);
    let app = App::new(state)
        .with_events(tx)
        .with_terminal_shell(Shell::script(script));
    let mut rig = Rig {
        app,
        events,
        terminal: Terminal::new(TestBackend::new(100, 30)).unwrap(),
        _dir: dir,
    };
    rig.draw();
    rig
}

impl Rig {
    fn draw(&mut self) {
        let app = &mut self.app;
        self.terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
    }

    fn press(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
        self.draw();
    }

    fn alt(&mut self, c: char) {
        self.press(KeyCode::Char(c), KeyModifiers::ALT);
    }

    /// Opens the terminal pane, which takes the keyboard, and waits for `text`.
    fn open_terminal_showing(&mut self, text: &str) {
        self.press(KeyCode::Char('`'), KeyModifiers::CONTROL);
        assert_eq!(self.app.state().focus(), Focus::Terminal);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            while let Ok(event) = self.events.try_recv() {
                self.app.handle_event(event);
            }
            self.draw();
            if self.terminal.backend().to_string().contains(text) {
                return;
            }
            assert!(Instant::now() < deadline, "timed out waiting for {text}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// From inside the terminal: the prefix, then the chord to the editor.
    fn terminal_to_editor(&mut self) {
        self.press(KeyCode::Char('b'), KeyModifiers::CONTROL);
        self.alt('k');
        assert_eq!(self.app.state().focus(), Focus::Editor);
    }

    /// The single cells with the dimmed cursor look: their neighbours do not
    /// have it, which tells them from a whole row such as the explorer's
    /// selection.
    fn dimmed_cursors(&self) -> Vec<(u16, u16)> {
        let dim = self.app.state().theme().unfocused_cursor().bg.unwrap();
        let buffer = self.terminal.backend().buffer();
        let area = buffer.area;
        let has = |x: u16, y: u16| x < area.width && buffer[(x, y)].bg == dim;
        let mut found = Vec::new();
        for y in 0..area.height {
            for x in 0..area.width {
                let alone = !x.checked_sub(1).is_some_and(|left| has(left, y)) && !has(x + 1, y);
                if has(x, y) && alone {
                    found.push((x, y));
                }
            }
        }
        found
    }
}

#[test]
fn the_editor_cursor_is_live_while_the_editor_has_the_keyboard() {
    let rig = rig("sleep 30");

    assert!(rig.dimmed_cursors().is_empty());
}

#[test]
fn the_editor_cursor_is_dimmed_while_the_explorer_has_the_keyboard() {
    let mut rig = rig("sleep 30");
    let live = rig.terminal.get_cursor_position().unwrap();

    rig.alt('h');

    assert_eq!(rig.app.state().focus(), Focus::Explorer);
    assert_eq!(rig.dimmed_cursors(), vec![(live.x, live.y)]);
}

#[test]
fn the_cursor_goes_live_again_when_the_editor_gets_the_keyboard_back() {
    let mut rig = rig("sleep 30");
    rig.alt('h');

    rig.alt('l');

    assert!(rig.dimmed_cursors().is_empty());
}

#[test]
fn the_editor_cursor_is_dimmed_while_the_find_bar_has_the_keyboard() {
    let mut rig = rig("sleep 30");
    let live = rig.terminal.get_cursor_position().unwrap();

    rig.press(KeyCode::Char('f'), KeyModifiers::CONTROL);

    assert!(!rig.app.state().has_keyboard(Focus::Editor));
    assert_eq!(rig.dimmed_cursors(), vec![(live.x, live.y)]);
}

#[test]
fn a_closed_find_bar_gives_the_keyboard_back() {
    let mut rig = rig("sleep 30");
    rig.press(KeyCode::Char('f'), KeyModifiers::CONTROL);

    rig.press(KeyCode::Esc, KeyModifiers::NONE);

    assert!(rig.app.state().has_keyboard(Focus::Editor));
    assert!(rig.dimmed_cursors().is_empty());
}

#[test]
fn a_split_dims_the_cursor_of_the_pane_without_the_keyboard() {
    let mut rig = rig("sleep 30");
    rig.app.state_mut().workspace_mut().split(Axis::Horizontal);
    rig.draw();

    // The new pane has the keyboard and the live cursor; the old one is dimmed.
    assert_eq!(rig.dimmed_cursors().len(), 1);

    rig.alt('h');
    rig.alt('h');

    assert_eq!(rig.app.state().focus(), Focus::Explorer);
    assert_eq!(rig.dimmed_cursors().len(), 2);
}

#[test]
fn the_terminal_cursor_is_dimmed_while_the_editor_has_the_keyboard() {
    let mut rig = rig("printf abc; sleep 30");
    rig.open_terminal_showing("abc");
    let live = rig.terminal.get_cursor_position().unwrap();
    assert_eq!(rig.dimmed_cursors().len(), 1, "only the editor is dimmed");

    rig.terminal_to_editor();

    assert_eq!(rig.dimmed_cursors(), vec![(live.x, live.y)]);
}

#[test]
fn a_terminal_cursor_the_program_hid_is_not_marked() {
    let mut rig = rig("printf 'abc\\033[?25l'; sleep 30");
    rig.open_terminal_showing("abc");

    rig.terminal_to_editor();

    assert!(rig.dimmed_cursors().is_empty());
}

#[test]
fn a_theme_without_the_slot_still_dims_the_cursor() {
    use crate::ui::theme::Theme;
    use ratatui::style::{Color, Style};

    let theme = Theme::from_toml("[\"tab.active\"]\ntext = \"#ffffff\"\n").unwrap();

    assert!(!theme.contains("cursor.unfocused"));
    assert_eq!(theme.unfocused_cursor(), Style::new().bg(Color::DarkGray));
}
