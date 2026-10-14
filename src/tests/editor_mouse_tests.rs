//! The editor's mouse beyond clicking and dragging in the text: line numbers,
//! the middle button and the right-click menu.

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

use crate::app::App;
use crate::app::state::AppState;
use crate::buffer::{Buffer, Position};
use crate::clipboard::{Clipboard, Memory, System};
use crate::components::editor::Editor;
use crate::components::menu::Entry;
use crate::components::prepare_and_render;

struct Screen {
    app: App,
    terminal: Terminal<TestBackend>,
}

fn screen(text: &str) -> Screen {
    screen_with_clipboard(text, Memory::default())
}

fn screen_with_clipboard(text: &str, memory: Memory) -> Screen {
    let editor = Editor::new(Buffer::from_text(text));
    let app =
        App::new(AppState::new(editor)).with_clipboard(Clipboard::new(System::Memory(memory)));
    let mut screen = Screen {
        app,
        terminal: Terminal::new(TestBackend::new(120, 40)).unwrap(),
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

    fn text_area(&self) -> Rect {
        self.app.state().editor().text_area().unwrap()
    }

    /// A cell in the line numbers of the line `line` (counting from the top of
    /// the screen).
    fn number(&self, line: u16) -> (u16, u16) {
        let area = self.text_area();
        (area.x - 2, area.y + line)
    }

    /// A cell in the text of the line `line`, `column` cells in.
    fn text(&self, line: u16, column: u16) -> (u16, u16) {
        let area = self.text_area();
        (area.x + column, area.y + line)
    }

    fn mouse(&mut self, kind: MouseEventKind, (column, row): (u16, u16), modifiers: KeyModifiers) {
        self.app.handle_input(InputEvent::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers,
        }));
        self.draw();
    }

    fn click(&mut self, at: (u16, u16)) {
        self.mouse(
            MouseEventKind::Down(MouseButton::Left),
            at,
            KeyModifiers::NONE,
        );
        self.mouse(
            MouseEventKind::Up(MouseButton::Left),
            at,
            KeyModifiers::NONE,
        );
    }

    fn press(&mut self, at: (u16, u16)) {
        self.mouse(
            MouseEventKind::Down(MouseButton::Left),
            at,
            KeyModifiers::NONE,
        );
    }

    fn drag(&mut self, at: (u16, u16)) {
        self.mouse(
            MouseEventKind::Drag(MouseButton::Left),
            at,
            KeyModifiers::NONE,
        );
    }

    fn release(&mut self, at: (u16, u16)) {
        self.mouse(
            MouseEventKind::Up(MouseButton::Left),
            at,
            KeyModifiers::NONE,
        );
    }

    fn right_click(&mut self, at: (u16, u16)) {
        self.mouse(
            MouseEventKind::Down(MouseButton::Right),
            at,
            KeyModifiers::NONE,
        );
    }

    fn middle_click(&mut self, at: (u16, u16)) {
        self.mouse(
            MouseEventKind::Down(MouseButton::Middle),
            at,
            KeyModifiers::NONE,
        );
    }

    fn key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
        self.draw();
    }

    fn selection(&self) -> (Position, Position) {
        let selection = self.app.state().editor().selections().primary();
        (selection.start(), selection.end())
    }

    fn head(&self) -> Position {
        self.app.state().editor().selections().primary().head()
    }

    fn text_of_buffer(&self) -> String {
        self.app.state().editor().buffer().text()
    }

    fn menu_labels(&self) -> Vec<&str> {
        self.app
            .state()
            .menu()
            .unwrap()
            .entries()
            .iter()
            .map(|entry| match entry {
                Entry::Item { label, .. } | Entry::Group { label, .. } => label.as_ref(),
                Entry::Separator => "---",
            })
            .collect()
    }
}

fn at(line: usize, column: usize) -> Position {
    Position::new(line, column)
}

const THREE_LINES: &str = "alpha\nbeta\ngamma";

// ---- line numbers

#[test]
fn clicking_a_line_number_selects_the_whole_line() {
    let mut screen = screen(THREE_LINES);

    screen.click(screen.number(1));

    assert_eq!(screen.selection(), (at(1, 0), at(2, 0)));
}

#[test]
fn the_last_line_is_selected_up_to_its_end() {
    let mut screen = screen(THREE_LINES);

    screen.click(screen.number(2));

    assert_eq!(screen.selection(), (at(2, 0), at(2, 5)));
}

#[test]
fn a_line_number_below_the_last_line_does_nothing() {
    let mut screen = screen(THREE_LINES);
    screen.click(screen.text(0, 2));

    screen.click(screen.number(10));

    assert_eq!(screen.selection(), (at(0, 2), at(0, 2)));
}

#[test]
fn shift_click_on_a_line_number_extends_down_to_that_line() {
    let mut screen = screen(THREE_LINES);
    screen.click(screen.text(0, 2));

    screen.mouse(
        MouseEventKind::Down(MouseButton::Left),
        screen.number(1),
        KeyModifiers::SHIFT,
    );

    assert_eq!(screen.selection(), (at(0, 2), at(2, 0)));
}

#[test]
fn dragging_from_a_line_number_selects_whole_lines_downward() {
    let mut screen = screen(THREE_LINES);

    screen.press(screen.number(0));
    screen.drag(screen.text(1, 3));

    assert_eq!(screen.selection(), (at(0, 0), at(2, 0)));
    screen.drag(screen.number(2));
    assert_eq!(screen.selection(), (at(0, 0), at(2, 5)));
}

#[test]
fn dragging_from_a_line_number_upward_keeps_the_pressed_line() {
    let mut screen = screen(THREE_LINES);

    screen.press(screen.number(2));
    screen.drag(screen.number(1));

    assert_eq!(screen.selection(), (at(1, 0), at(2, 5)));
    screen.drag(screen.number(0));
    assert_eq!(screen.selection(), (at(0, 0), at(2, 5)));
}

#[test]
fn dragging_below_the_text_selects_to_the_last_line() {
    let mut screen = screen(THREE_LINES);

    screen.press(screen.number(0));
    screen.drag(screen.number(30));

    assert_eq!(screen.selection(), (at(0, 0), at(2, 5)));
}

#[test]
fn after_the_release_a_drag_no_longer_selects_lines() {
    let mut screen = screen(THREE_LINES);
    screen.press(screen.number(0));
    screen.release(screen.number(0));

    screen.drag(screen.number(2));

    assert_eq!(screen.selection(), (at(0, 0), at(1, 0)));
}

#[test]
fn a_click_in_the_text_after_a_line_number_click_is_an_ordinary_click() {
    let mut screen = screen(THREE_LINES);
    screen.click(screen.number(0));

    screen.press(screen.text(1, 2));
    screen.drag(screen.text(1, 4));

    assert_eq!(screen.selection(), (at(1, 2), at(1, 4)));
}

#[test]
fn typing_replaces_the_line_that_was_selected_by_its_number() {
    let mut screen = screen(THREE_LINES);
    screen.click(screen.number(1));

    screen.key(KeyCode::Char('x'), KeyModifiers::NONE);

    assert_eq!(screen.text_of_buffer(), "alpha\nxgamma");
}

#[test]
fn the_line_number_clicked_is_the_one_on_screen_when_scrolled() {
    let text: String = (0..100).map(|n| format!("line {n}\n")).collect();
    let mut screen = screen(&text);
    screen
        .app
        .state_mut()
        .workspace_mut()
        .with_editor(|editor| {
            editor.scroll_lines(5);
        });
    screen.draw();

    screen.click(screen.number(2));

    // Scrolled by 5 notches of 3 lines: the screen's third row is line 17.
    assert_eq!(screen.selection(), (at(17, 0), at(18, 0)));
}

// ---- middle button

#[test]
fn middle_click_in_the_text_pastes_there() {
    let mut screen = screen_with_clipboard(
        "alpha\nbeta",
        Memory {
            content: Some("XY".to_owned()),
            ..Memory::default()
        },
    );

    screen.middle_click(screen.text(1, 2));

    assert_eq!(screen.text_of_buffer(), "alpha\nbeXYta");
    assert_eq!(screen.head(), at(1, 4));
}

#[test]
fn middle_click_with_nothing_to_paste_says_so() {
    let mut screen = screen("alpha");

    screen.middle_click(screen.text(0, 2));

    assert_eq!(screen.text_of_buffer(), "alpha");
    assert_eq!(
        screen.app.state().latest_notification(),
        Some("nothing to paste")
    );
}

#[test]
fn middle_click_on_a_line_number_pastes_nothing() {
    let mut screen = screen_with_clipboard(
        "alpha",
        Memory {
            content: Some("XY".to_owned()),
            ..Memory::default()
        },
    );

    screen.middle_click(screen.number(0));

    assert_eq!(screen.text_of_buffer(), "alpha");
}

#[test]
fn middle_click_replaces_nothing_even_with_a_selection_elsewhere() {
    let mut screen = screen_with_clipboard(
        "alpha\nbeta",
        Memory {
            content: Some("XY".to_owned()),
            ..Memory::default()
        },
    );
    screen.click(screen.number(0));

    screen.middle_click(screen.text(1, 0));

    assert_eq!(screen.text_of_buffer(), "alpha\nXYbeta");
}

// ---- right-click menu

#[test]
fn right_click_in_the_text_moves_the_cursor_and_opens_the_menu() {
    let mut screen = screen(THREE_LINES);

    screen.right_click(screen.text(1, 2));

    assert_eq!(screen.head(), at(1, 2));
    assert_eq!(
        screen.menu_labels(),
        [
            "Cut",
            "Copy",
            "Paste",
            "---",
            "Select All",
            "---",
            "Find in File",
            "Save"
        ]
    );
    assert_eq!(screen.app.state().menu().unwrap().target(), None);
}

#[test]
fn right_click_inside_the_selection_keeps_it() {
    let mut screen = screen(THREE_LINES);
    screen.click(screen.number(1));

    screen.right_click(screen.text(1, 2));

    assert_eq!(screen.selection(), (at(1, 0), at(2, 0)));
    assert!(screen.app.state().menu().is_some());
}

#[test]
fn right_click_outside_the_selection_puts_the_cursor_there() {
    let mut screen = screen(THREE_LINES);
    screen.click(screen.number(1));

    screen.right_click(screen.text(0, 1));

    assert_eq!(screen.selection(), (at(0, 1), at(0, 1)));
}

#[test]
fn right_click_on_a_line_number_opens_no_menu() {
    let mut screen = screen(THREE_LINES);

    screen.right_click(screen.number(1));

    assert!(screen.app.state().menu().is_none());
}

#[test]
fn the_menu_items_show_their_keys() {
    let mut screen = screen(THREE_LINES);

    screen.right_click(screen.text(0, 0));

    let keys: Vec<Option<String>> = screen
        .app
        .state()
        .menu()
        .unwrap()
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            Entry::Item { keys, .. } => Some(keys.clone()),
            Entry::Group { .. } | Entry::Separator => None,
        })
        .collect();
    assert_eq!(keys[0].as_deref(), Some("ctrl+x"));
    assert_eq!(keys[1].as_deref(), Some("ctrl+c"));
    assert_eq!(keys[2].as_deref(), Some("ctrl+v"));
}

#[test]
fn choosing_select_all_from_the_menu_selects_everything() {
    let mut screen = screen(THREE_LINES);
    screen.right_click(screen.text(0, 0));
    // Cut, Copy, Paste, then Select All (the separator is skipped).
    for _ in 0..3 {
        screen.key(KeyCode::Down, KeyModifiers::NONE);
    }

    screen.key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(screen.app.state().menu().is_none());
    assert_eq!(screen.selection(), (at(0, 0), at(2, 5)));
}

#[test]
fn choosing_cut_removes_the_selection_and_keeps_it_for_pasting() {
    let mut screen = screen(THREE_LINES);
    screen.click(screen.number(1));
    screen.right_click(screen.text(1, 1));

    screen.key(KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(screen.text_of_buffer(), "alpha\ngamma");
    screen.middle_click(screen.text(0, 0));
    assert_eq!(screen.text_of_buffer(), "beta\nalpha\ngamma");
}

#[test]
fn clicking_an_item_with_the_mouse_runs_it() {
    let mut screen = screen_with_clipboard(
        "alpha",
        Memory {
            content: Some("XY".to_owned()),
            ..Memory::default()
        },
    );
    screen.right_click(screen.text(0, 5));
    let screen_text = screen.terminal.backend().to_string();
    let (row, line) = screen_text
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("Paste"))
        .unwrap();
    let column = line.chars().take_while(|_| true).count();
    let column = line
        .find("Paste")
        .map(|b| line[..b].chars().count())
        .unwrap_or(column);

    screen.click((column as u16, row as u16));

    assert_eq!(screen.text_of_buffer(), "alphaXY");
    assert!(screen.app.state().menu().is_none());
}

#[test]
fn escape_closes_the_menu_and_changes_nothing() {
    let mut screen = screen(THREE_LINES);
    screen.right_click(screen.text(0, 2));

    screen.key(KeyCode::Esc, KeyModifiers::NONE);

    assert!(screen.app.state().menu().is_none());
    assert_eq!(screen.text_of_buffer(), THREE_LINES);
}

#[test]
fn shift_f10_opens_the_menu_at_the_cursor() {
    let mut screen = screen(THREE_LINES);
    screen.click(screen.text(1, 3));

    screen.key(KeyCode::F(10), KeyModifiers::SHIFT);

    assert!(screen.app.state().menu().is_some());
    assert_eq!(screen.menu_labels()[0], "Cut");
    assert_eq!(screen.head(), at(1, 3));
}

#[test]
fn shift_f10_in_the_explorer_still_opens_the_explorer_menu() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "").unwrap();
    let mut state = AppState::default();
    state.open_project(dir.path()).unwrap();
    let mut screen = Screen {
        app: App::new(state),
        terminal: Terminal::new(TestBackend::new(120, 40)).unwrap(),
    };
    screen.draw();
    screen.key(KeyCode::Char('b'), KeyModifiers::CONTROL);

    screen.key(KeyCode::F(10), KeyModifiers::SHIFT);

    assert!(screen.menu_labels().contains(&"New File"));
    assert!(screen.app.state().menu().unwrap().target().is_some());
}

#[test]
fn right_click_while_the_explorer_has_the_keyboard_gives_it_to_the_editor() {
    let mut screen = screen(THREE_LINES);
    screen.key(KeyCode::Char('b'), KeyModifiers::CONTROL);

    screen.right_click(screen.text(0, 1));
    screen.key(KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!(screen.app.state().focus(), crate::app::state::Focus::Editor);
}
