//! The theme's `[background]` reaches every cell: the editor, the boxes drawn
//! over it and the terminal pane, whatever the shell says about its own colors.

use std::fs;
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
use crate::ui::theme::Theme;

const SOLID: &str = "[background]\ncolor = \"#102030\"\n";
const PAINTED: Color = Color::Rgb(0x10, 0x20, 0x30);

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    terminal: Terminal<TestBackend>,
    _dir: tempfile::TempDir,
}

fn rig(theme: &str, script: &str) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.txt");
    fs::write(&file, "alpha\nbeta\n").unwrap();
    let mut state = AppState::default();
    state.set_theme(Theme::from_toml(theme).unwrap());
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

    fn wait_for_text(&mut self, text: &str) {
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

    /// The backgrounds that are not `expected`, with where they are. The
    /// dimmed cursor of a place without the keyboard is one cell and is allowed.
    fn strays(&self, expected: Color) -> Vec<(u16, u16, Color)> {
        let buffer = self.terminal.backend().buffer();
        let width = buffer.area.width;
        buffer
            .content()
            .iter()
            .enumerate()
            .filter(|(_, cell)| cell.bg != expected && cell.bg != Color::DarkGray)
            .map(|(at, cell)| ((at as u16) % width, (at as u16) / width, cell.bg))
            .collect()
    }
}

#[test]
fn the_editor_has_the_painted_background() {
    let rig = rig(SOLID, "sleep 30");

    assert_eq!(rig.strays(PAINTED), vec![]);
}

#[test]
fn a_box_over_the_editor_has_the_painted_background() {
    let mut rig = rig(SOLID, "sleep 30");

    rig.press(KeyCode::F(1), KeyModifiers::NONE);

    assert!(
        rig.terminal
            .backend()
            .to_string()
            .contains("Command Palette")
    );
    assert_eq!(rig.strays(PAINTED), vec![]);
}

#[test]
fn the_file_finder_and_the_project_search_have_the_painted_background() {
    for (keys, title) in [
        (KeyCode::Char('p'), "Go to File"),
        (KeyCode::Char('f'), "Find in Project"),
    ] {
        let modifiers = match title {
            "Go to File" => KeyModifiers::CONTROL,
            _ => KeyModifiers::ALT,
        };
        let mut rig = rig(SOLID, "sleep 30");

        rig.press(keys, modifiers);

        assert!(
            rig.terminal.backend().to_string().contains(title),
            "{title}"
        );
        assert_eq!(rig.strays(PAINTED), vec![], "{title}");
    }
}

#[test]
fn the_terminal_pane_shows_the_painted_background_in_cells_with_the_default_one() {
    let mut rig = rig(SOLID, "printf 'abc'; sleep 30");
    rig.press(KeyCode::Char('`'), KeyModifiers::CONTROL);
    rig.wait_for_text("abc");

    assert_eq!(rig.strays(PAINTED), vec![]);
}

#[test]
fn a_color_the_shell_sets_still_wins_in_the_terminal_pane() {
    let mut rig = rig(SOLID, "printf '\\033[41mred\\033[0m'; sleep 30");
    rig.press(KeyCode::Char('`'), KeyModifiers::CONTROL);
    rig.wait_for_text("red");

    let strays = rig.strays(PAINTED);

    assert_eq!(strays.len(), 3, "{strays:?}");
    assert!(strays.iter().all(|(_, _, bg)| *bg == Color::Indexed(1)));
}

#[test]
fn a_transparent_background_paints_nothing_in_a_box_or_the_terminal_pane() {
    let transparent = "[background]\ncolor = \"transparent\"\n";
    let mut rig = rig(transparent, "printf 'abc'; sleep 30");
    rig.press(KeyCode::Char('`'), KeyModifiers::CONTROL);
    rig.wait_for_text("abc");
    assert_eq!(rig.strays(Color::Reset), vec![], "terminal pane");

    rig.press(KeyCode::F(1), KeyModifiers::NONE);

    assert_eq!(rig.strays(Color::Reset), vec![], "palette");
}

const TERMINAL_DARKER: &str =
    "[background]\ncolor = \"#102030\"\n[background.terminal]\ncolor = \"#000000\"\n";
const DARKER: Color = Color::Rgb(0, 0, 0);

#[test]
fn a_pane_with_its_own_background_differs_from_the_rest() {
    let mut rig = rig(TERMINAL_DARKER, "printf 'abc'; sleep 30");
    rig.press(KeyCode::Char('`'), KeyModifiers::CONTROL);
    rig.wait_for_text("abc");

    let strays = rig.strays(PAINTED);

    assert!(!strays.is_empty());
    assert!(strays.iter().all(|(_, _, bg)| *bg == DARKER), "{strays:?}");
    // The terminal pane is at the bottom; nothing above the middle is touched.
    assert!(strays.iter().all(|(_, y, _)| *y > 15), "{strays:?}");
}

#[test]
fn a_pane_transparent_on_its_own_leaves_the_others_painted() {
    let theme =
        "[background]\ncolor = \"#102030\"\n[background.explorer]\ncolor = \"transparent\"\n";
    let rig = rig(theme, "sleep 30");

    let strays = rig.strays(PAINTED);

    assert!(!strays.is_empty());
    assert!(
        strays.iter().all(|(_, _, bg)| *bg == Color::Reset),
        "{strays:?}"
    );
    assert!(strays.iter().all(|(x, _, _)| *x < 40), "{strays:?}");
}

#[test]
fn the_popup_table_colors_every_box() {
    let theme = "[background]\ncolor = \"#102030\"\n[background.popup]\ncolor = \"#000000\"\n";
    let mut rig = rig(theme, "sleep 30");

    rig.press(KeyCode::F(1), KeyModifiers::NONE);

    let strays = rig.strays(PAINTED);
    assert!(!strays.is_empty());
    assert!(strays.iter().all(|(_, _, bg)| *bg == DARKER), "{strays:?}");
}
