//! Moving tabs along the strip, by key and by drag, and the wheel over the strip. A pinned tab
//! stays among the pinned ones and a loose tab among the loose.

use std::fs;

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::app::App;
use crate::components::prepare_and_render;
use crate::event::action::Action;
use crate::initial_state;

struct Rig {
    app: App,
    _dir: tempfile::TempDir,
}

/// An app with `count` files open, `f1.txt` to `fN.txt`, the first in front.
fn rig(count: usize) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let files: Vec<_> = (1..=count)
        .map(|n| {
            let path = dir.path().join(format!("f{n}.txt"));
            fs::write(&path, format!("text {n}\n")).unwrap();
            path
        })
        .collect();
    let mut state = initial_state(&files).unwrap();
    state.open_project(dir.path()).unwrap();
    let mut rig = Rig {
        app: App::new(state),
        _dir: dir,
    };
    rig.draw();
    rig
}

impl Rig {
    fn draw(&mut self) -> String {
        let mut terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
        let app = &mut self.app;
        terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
        terminal.backend().to_string()
    }

    fn names(&self) -> Vec<String> {
        self.app.state().workspace().tab_names_for_test()
    }

    fn front(&self) -> String {
        self.app.state().editor().display_name()
    }

    /// The cell where the tab called `name` starts, in the tab strip (the row under the menu bar).
    fn tab_cell(&mut self, name: &str) -> (u16, u16) {
        let screen = self.draw();
        for (row, line) in screen.lines().enumerate() {
            let line = line.trim_matches('"');
            if line.contains('≡') {
                continue;
            }
            if let Some(byte) = line.find(name) {
                return (line[..byte].chars().count() as u16 + 1, row as u16);
            }
        }
        panic!("no tab {name}: {screen}");
    }

    fn mouse(&mut self, kind: MouseEventKind, cell: (u16, u16)) {
        self.app.handle_input(InputEvent::Mouse(MouseEvent {
            kind,
            column: cell.0,
            row: cell.1,
            modifiers: KeyModifiers::NONE,
        }));
        self.draw();
    }

    fn drag(&mut self, from: (u16, u16), to: &[(u16, u16)]) {
        self.mouse(MouseEventKind::Down(MouseButton::Left), from);
        for cell in to {
            self.mouse(MouseEventKind::Drag(MouseButton::Left), *cell);
        }
        let end = *to.last().unwrap_or(&from);
        self.mouse(MouseEventKind::Up(MouseButton::Left), end);
    }

    fn pin_front(&mut self) {
        self.app.run_action(Action::TogglePinTab);
        self.draw();
    }
}

#[test]
fn a_key_moves_the_tab_in_front_one_place() {
    let mut rig = rig(4);
    rig.app.state_mut().workspace_mut().activate_tab(1);

    rig.app.run_action(Action::MoveTabRight);
    assert_eq!(rig.names(), ["f1.txt", "f3.txt", "f2.txt", "f4.txt"]);
    assert_eq!(
        rig.front(),
        "f2.txt",
        "the tab moved is still the one in front"
    );

    rig.app.run_action(Action::MoveTabLeft);
    rig.app.run_action(Action::MoveTabLeft);
    assert_eq!(rig.names(), ["f2.txt", "f1.txt", "f3.txt", "f4.txt"]);
}

#[test]
fn the_keys_stop_at_the_ends_of_the_strip() {
    let mut rig = rig(3);

    rig.app.run_action(Action::MoveTabLeft);
    assert_eq!(rig.names(), ["f1.txt", "f2.txt", "f3.txt"]);

    rig.app.state_mut().workspace_mut().activate_tab(2);
    rig.app.run_action(Action::MoveTabRight);
    assert_eq!(rig.names(), ["f1.txt", "f2.txt", "f3.txt"]);
}

#[test]
fn ctrl_shift_page_down_and_up_are_bound_to_moving_the_tab() {
    let mut rig = rig(3);
    let press = |rig: &mut Rig, code| {
        rig.app.handle_input(InputEvent::Key(KeyEvent::new(
            code,
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        )));
    };

    press(&mut rig, KeyCode::PageDown);
    assert_eq!(rig.names(), ["f2.txt", "f1.txt", "f3.txt"]);

    press(&mut rig, KeyCode::PageUp);
    assert_eq!(rig.names(), ["f1.txt", "f2.txt", "f3.txt"]);
}

#[test]
fn a_pinned_tab_does_not_move_out_of_the_pinned_ones_and_a_loose_one_does_not_move_in() {
    let mut rig = rig(4);
    // f1 and f2 pinned (pinning moves a tab to the end of the pinned ones).
    rig.pin_front();
    rig.app.state_mut().workspace_mut().activate_tab(1);
    rig.pin_front();
    assert_eq!(rig.names(), ["f1.txt", "f2.txt", "f3.txt", "f4.txt"]);

    // The pinned f2 cannot go right past the line...
    rig.app.state_mut().workspace_mut().activate_tab(1);
    rig.app.run_action(Action::MoveTabRight);
    assert_eq!(rig.names(), ["f1.txt", "f2.txt", "f3.txt", "f4.txt"]);
    // ...but moves among the pinned.
    rig.app.run_action(Action::MoveTabLeft);
    assert_eq!(rig.names(), ["f2.txt", "f1.txt", "f3.txt", "f4.txt"]);

    // A loose tab cannot go left into them.
    rig.app.state_mut().workspace_mut().activate_tab(2);
    rig.app.run_action(Action::MoveTabLeft);
    assert_eq!(rig.names(), ["f2.txt", "f1.txt", "f3.txt", "f4.txt"]);
    assert!(!rig.app.state().workspace().active_tab_is_pinned());
}

#[test]
fn dragging_a_tab_over_the_next_one_swaps_them() {
    let mut rig = rig(3);
    let (x1, y) = rig.tab_cell("f1.txt");
    let (x2, _) = rig.tab_cell("f2.txt");

    rig.drag((x1, y), &[(x2 + 1, y)]);

    assert_eq!(rig.names(), ["f2.txt", "f1.txt", "f3.txt"]);
    assert_eq!(rig.front(), "f1.txt");
}

#[test]
fn a_drag_goes_as_far_as_the_pointer_and_stops_at_the_strips_end() {
    let mut rig = rig(4);
    let (x1, y) = rig.tab_cell("f1.txt");
    let (x4, _) = rig.tab_cell("f4.txt");

    // Over f2, f3, f4, and then far past the last tab.
    let (x2, _) = rig.tab_cell("f2.txt");
    let (x3, _) = rig.tab_cell("f3.txt");
    rig.mouse(MouseEventKind::Down(MouseButton::Left), (x1, y));
    for column in [x2 + 1, x3 + 1, x4 + 1, 115] {
        rig.mouse(MouseEventKind::Drag(MouseButton::Left), (column, y));
    }
    rig.mouse(MouseEventKind::Up(MouseButton::Left), (115, y));

    assert_eq!(rig.names(), ["f2.txt", "f3.txt", "f4.txt", "f1.txt"]);
}

#[test]
fn a_drag_takes_a_loose_tab_to_the_line_and_not_over_the_pinned_one() {
    let mut rig = rig(3);
    rig.pin_front();
    let (x1, y) = rig.tab_cell("f1.txt");
    let (x3, _) = rig.tab_cell("f3.txt");

    // Drag f3 left over the pinned f1: it stops first among the loose ones.
    rig.drag((x3, y), &[(x1 + 1, y)]);

    assert_eq!(rig.names(), ["f1.txt", "f3.txt", "f2.txt"]);
    assert!(!rig.app.state().workspace().active_tab_is_pinned());
}

#[test]
fn a_plain_click_on_a_tab_changes_nothing_but_the_tab_in_front() {
    let mut rig = rig(3);
    let (x2, y) = rig.tab_cell("f2.txt");

    rig.drag((x2, y), &[]);

    assert_eq!(rig.names(), ["f1.txt", "f2.txt", "f3.txt"]);
    assert_eq!(rig.front(), "f2.txt");
}

#[test]
fn a_drag_that_began_in_the_text_does_not_move_tabs() {
    let mut rig = rig(3);
    let (x1, y) = rig.tab_cell("f1.txt");
    let (x2, _) = rig.tab_cell("f2.txt");

    rig.mouse(MouseEventKind::Down(MouseButton::Left), (x1, y + 3));
    rig.mouse(MouseEventKind::Drag(MouseButton::Left), (x2, y));
    rig.mouse(MouseEventKind::Up(MouseButton::Left), (x2, y));

    assert_eq!(rig.names(), ["f1.txt", "f2.txt", "f3.txt"]);
}

#[test]
fn the_wheel_over_the_strip_steps_through_the_tabs_without_going_round() {
    let mut rig = rig(3);
    let (x1, y) = rig.tab_cell("f1.txt");

    rig.mouse(MouseEventKind::ScrollDown, (x1, y));
    assert_eq!(rig.front(), "f2.txt");
    rig.mouse(MouseEventKind::ScrollDown, (x1, y));
    rig.mouse(MouseEventKind::ScrollDown, (x1, y));
    assert_eq!(rig.front(), "f3.txt", "it stops at the last");

    rig.mouse(MouseEventKind::ScrollUp, (x1, y));
    assert_eq!(rig.front(), "f2.txt");
    rig.mouse(MouseEventKind::ScrollUp, (x1, y));
    rig.mouse(MouseEventKind::ScrollUp, (x1, y));
    assert_eq!(rig.front(), "f1.txt", "it stops at the first");
}

#[test]
fn the_wheel_over_the_text_still_scrolls_the_text_and_not_the_tabs() {
    let mut rig = rig(3);
    let (x1, y) = rig.tab_cell("f1.txt");

    rig.mouse(MouseEventKind::ScrollDown, (x1, y + 4));

    assert_eq!(rig.front(), "f1.txt");
}

#[test]
fn the_wheel_brings_a_tab_that_was_out_of_the_strip_into_it() {
    let mut rig = rig(30);
    let (x1, y) = rig.tab_cell("f1.txt");

    for _ in 0..29 {
        rig.mouse(MouseEventKind::ScrollDown, (x1, y));
    }

    assert_eq!(rig.front(), "f30.txt");
    assert!(rig.draw().contains("f30.txt"), "the strip followed");
}
