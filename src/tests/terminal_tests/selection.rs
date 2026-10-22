//! Copying from the terminal pane: a drag selects text (a plain one when the program has not
//! asked for the mouse, `shift` held when it has), letting go copies it, a double click takes a
//! word and a triple click a line, `ctrl+shift+c` and `ctrl+b y` copy again, and a press, typing,
//! another tab, the history moving or the text changing drops it.

use crossterm::event::MouseButton::Left;
use crossterm::event::MouseEventKind::{Down, Drag, Up};

use super::*;

const TEXT: &str = "printf 'alpha beta\\ngamma delta\\n'; sleep 30";

/// A shell that shows two lines and then what it is sent, with the mouse on.
const TEXT_MOUSE_ON: &str =
    "printf 'alpha beta\\ngamma delta\\n\\033[?1000h\\033[?1006h'; stty raw -echo; cat -v";

/// A shell that shows two lines and then shows the bytes it gets.
const TEXT_THEN_BYTES: &str = "printf 'alpha beta\\ngamma delta\\n'; stty raw -echo; cat -v";

fn shown(script: &str) -> (Rig, tempfile::TempDir) {
    let (mut rig, dir) = rig(script);
    // A clipboard that works here, standing in for the system's.
    let clipboard = crate::clipboard::Clipboard::new(crate::clipboard::System::Memory(
        crate::clipboard::Memory::default(),
    ));
    let app = std::mem::take(&mut rig.app);
    rig.app = app.with_clipboard(clipboard);
    rig.toggle();
    rig.wait_for_text("gamma delta");
    std::thread::sleep(Duration::from_millis(200));
    rig.draw();
    (rig, dir)
}

impl Rig {
    /// A drag from cell (`from`) to cell (`to`) of the shell's text, as (column, row) of it.
    fn drag(&mut self, from: (u16, u16), to: (u16, u16), modifiers: KeyModifiers) {
        let (ox, oy) = self.text_origin();
        let at = |cell: (u16, u16)| (ox + cell.0, oy + cell.1);
        let (fx, fy) = at(from);
        let (tx, ty) = at(to);
        self.app.handle_input(mouse(Down(Left), fx, fy, modifiers));
        self.app.handle_input(mouse(Drag(Left), tx, ty, modifiers));
        self.app.handle_input(mouse(Up(Left), tx, ty, modifiers));
        self.draw();
    }

    fn click(&mut self, cell: (u16, u16)) {
        let (ox, oy) = self.text_origin();
        let (x, y) = (ox + cell.0, oy + cell.1);
        self.app
            .handle_input(mouse(Down(Left), x, y, KeyModifiers::NONE));
        self.app
            .handle_input(mouse(Up(Left), x, y, KeyModifiers::NONE));
        self.draw();
    }

    fn selected(&self) -> Option<String> {
        self.app
            .state()
            .terminal()
            .selected_text()
            .map(str::to_owned)
    }

    /// What the clipboard holds, by pasting it into the editor.
    fn clipboard(&mut self) -> String {
        self.app.state_mut().set_focus(Focus::Editor);
        self.press(KeyCode::Char('a'), KeyModifiers::CONTROL);
        self.press(KeyCode::Char('v'), KeyModifiers::CONTROL);
        self.app.state().editor().buffer().text()
    }
}

#[test]
fn a_plain_drag_selects_text_and_letting_go_copies_it_quietly() {
    let (mut rig, _dir) = shown(TEXT);

    rig.drag((0, 0), (4, 0), KeyModifiers::NONE);

    assert_eq!(rig.selected().as_deref(), Some("alpha"));
    assert_eq!(rig.notifications(), "", "a copy on letting go says nothing");
    assert_eq!(rig.clipboard(), "alpha");
}

#[test]
fn a_drag_over_several_rows_takes_the_rows_between() {
    let (mut rig, _dir) = shown(TEXT);

    rig.drag((6, 0), (4, 1), KeyModifiers::NONE);

    assert_eq!(rig.selected().as_deref(), Some("beta\ngamma"));
    assert_eq!(rig.clipboard(), "beta\ngamma");
}

#[test]
fn a_drag_backwards_or_out_of_the_text_selects_to_the_edge() {
    let (mut rig, _dir) = shown(TEXT);
    rig.drag((4, 0), (0, 0), KeyModifiers::NONE);
    assert_eq!(rig.selected().as_deref(), Some("alpha"));

    // Past the right edge and below the text: as far as the screen goes.
    let (ox, oy) = rig.text_origin();
    rig.app
        .handle_input(mouse(Down(Left), ox + 6, oy, KeyModifiers::NONE));
    rig.app
        .handle_input(mouse(Drag(Left), 99, 29, KeyModifiers::NONE));
    rig.app
        .handle_input(mouse(Up(Left), 99, 29, KeyModifiers::NONE));
    let text = rig.selected().unwrap();
    assert!(text.starts_with("beta\ngamma delta"), "{text:?}");
}

#[test]
fn a_double_click_takes_a_word_and_a_triple_click_the_line() {
    let (mut rig, _dir) = shown(TEXT);
    let (ox, oy) = rig.text_origin();
    let on_beta = (ox + 7, oy);

    rig.app
        .handle_input(mouse(Down(Left), on_beta.0, on_beta.1, KeyModifiers::NONE));
    rig.app
        .handle_input(mouse(Up(Left), on_beta.0, on_beta.1, KeyModifiers::NONE));
    rig.app
        .handle_input(mouse(Down(Left), on_beta.0, on_beta.1, KeyModifiers::NONE));
    assert_eq!(rig.selected().as_deref(), Some("beta"));
    assert_eq!(rig.clipboard(), "beta");

    rig.app.state_mut().set_focus(Focus::Terminal);
    rig.app
        .handle_input(mouse(Up(Left), on_beta.0, on_beta.1, KeyModifiers::NONE));
    rig.app
        .handle_input(mouse(Down(Left), on_beta.0, on_beta.1, KeyModifiers::NONE));
    assert_eq!(rig.selected().as_deref(), Some("alpha beta"));
}

#[test]
fn the_selected_cells_are_drawn_as_a_selection() {
    let (mut rig, _dir) = shown(TEXT);
    rig.drag((0, 0), (4, 0), KeyModifiers::NONE);
    rig.draw();
    let (ox, oy) = rig.text_origin();
    let style = |column: u16| {
        rig.terminal
            .backend()
            .buffer()
            .cell((ox + column, oy))
            .unwrap()
            .style()
    };
    assert_ne!(
        style(0),
        style(6),
        "`alpha` is highlighted and ` beta` is not"
    );
    assert_eq!(style(0), style(4));
}

#[test]
fn a_press_drops_the_selection_and_typing_does_too() {
    let (mut rig, _dir) = shown(TEXT_THEN_BYTES);
    rig.drag((0, 0), (4, 0), KeyModifiers::NONE);
    assert!(rig.selected().is_some());

    rig.click((8, 1));
    assert_eq!(rig.selected(), None, "a click selects nothing");

    rig.drag((0, 0), (4, 0), KeyModifiers::NONE);
    assert!(rig.selected().is_some());
    rig.app.state_mut().set_focus(Focus::Terminal);
    rig.press(KeyCode::Char('x'), KeyModifiers::NONE);
    assert_eq!(rig.selected(), None, "typing is done with it");
}

#[test]
fn a_program_that_has_the_mouse_gets_a_plain_drag_and_shift_drag_selects() {
    let (mut rig, _dir) = shown(TEXT_MOUSE_ON);

    rig.drag((0, 0), (4, 0), KeyModifiers::NONE);
    assert_eq!(rig.selected(), None, "the program has the drag");
    rig.wait_for("the program to have been told", |rig| {
        rig.screen().contains("<0;")
    });

    rig.drag((0, 0), (4, 0), KeyModifiers::SHIFT);
    assert_eq!(rig.selected().as_deref(), Some("alpha"));
}

#[test]
fn ctrl_shift_c_and_the_prefix_y_copy_again_and_say_so() {
    let (mut rig, _dir) = shown(TEXT_THEN_BYTES);
    rig.press(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert_eq!(rig.notifications(), "nothing is selected in the terminal");

    rig.drag((0, 0), (4, 0), KeyModifiers::NONE);
    rig.app.state_mut().set_focus(Focus::Terminal);
    rig.press(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert_eq!(rig.notifications(), "copied 5 characters");

    rig.app.state_mut().set_focus(Focus::Terminal);
    rig.prefixed('y');
    assert_eq!(rig.notifications(), "copied 5 characters");
}

#[test]
fn moving_into_the_history_drops_the_selection() {
    let (mut rig, _dir) = rig_with_history();
    rig.drag((0, 0), (2, 0), KeyModifiers::NONE);
    assert!(rig.selected().is_some());
    rig.app.state_mut().set_focus(Focus::Terminal);

    rig.press(KeyCode::PageUp, KeyModifiers::SHIFT);
    rig.draw();

    assert_eq!(rig.selected(), None);
}

#[test]
fn the_palette_has_a_command_for_it() {
    let titles: Vec<_> = crate::event::action::Action::palette_actions()
        .iter()
        .filter_map(|action| action.title())
        .collect();
    assert!(titles.contains(&"Terminal: Copy Selection"));
}
