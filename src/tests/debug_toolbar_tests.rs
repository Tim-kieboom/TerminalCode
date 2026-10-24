//! The toolbar of the debugger over the editor: when it is there and what its buttons do.

use crossterm::event::{
    Event as InputEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use serde_json::Value;

use super::debug_hover_tests::stopped;
use super::debug_tests::debugging;
use super::run_tests::Rig;
use crate::components::debug::model::Status;

impl Rig {
    /// A left click on the button that shows `glyph` on the row of the toolbar.
    fn click_toolbar(&mut self, glyph: &str) {
        let row = self.row_of("paused");
        let column =
            u16::try_from(self.gutter_of(&format!(" {glyph} ")).chars().count() + 1).unwrap();
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
        ] {
            self.app.handle_input(InputEvent::Mouse(MouseEvent {
                kind,
                column,
                row,
                modifiers: KeyModifiers::NONE,
            }));
        }
        self.draw();
    }
}

#[test]
fn there_is_no_toolbar_without_a_session() {
    let mut rig = debugging(|_, _| {});
    rig.open_file("main.rs");
    rig.draw();

    let screen = rig.screen();

    assert!(
        !screen.contains("paused") && !screen.contains('■'),
        "{screen}"
    );
}

#[test]
fn while_the_program_is_stopped_the_toolbar_says_paused_with_a_button_for_each_control() {
    let mut rig = stopped(|_, _| {});
    rig.draw();

    let screen = rig.screen();

    assert!(screen.contains("paused"), "{screen}");
    for glyph in ["▶", "↷", "↓", "↑", "↻", "■"] {
        assert!(
            screen.contains(glyph),
            "{glyph} is not on the screen: {screen}"
        );
    }
    // In the middle of the editor, on its first line of text (the tab bar is above it).
    let toolbar = rig.row_of("paused");
    assert_eq!(toolbar, rig.row_of("main.rs") + 1, "{screen}");
}

#[test]
fn the_buttons_step_and_stop_the_program() {
    let mut rig = stopped(|adapter, _| {
        let (next, _) = adapter.expect_request("next");
        adapter.respond(next, "next", Value::Null);
        let (bye, arguments) = adapter.expect_request("disconnect");
        assert_eq!(arguments["terminateDebuggee"], true);
        adapter.respond(bye, "disconnect", Value::Null);
    });
    rig.draw();
    assert!(matches!(rig.status(), Status::Stopped { .. }));

    rig.click_toolbar("↷");
    assert_eq!(
        rig.status(),
        Status::Running,
        "the step went to the adapter"
    );
    // The running program says so, and its stepping buttons are not for use.
    rig.draw();
    assert!(rig.screen().contains("running"), "{}", rig.screen());

    // The program is running: its toolbar still has the stop.
    let row = rig.row_of("running");
    let column = u16::try_from(rig.gutter_of(" ■ ").chars().count() + 1).unwrap();
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        rig.app.handle_input(InputEvent::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    }

    rig.wait_for_status("the end", |status| matches!(status, Status::Ended(_)));
    rig.draw();
    assert!(!rig.screen().contains("running"), "{}", rig.screen());
}

#[test]
fn a_click_on_the_toolbar_does_not_move_the_cursor_of_the_text_under_it() {
    let mut rig = stopped(|_, _| {});
    rig.draw();
    let before = rig.app.state().editor().selections().primary().head();

    // Between the buttons: on the toolbar, on no button.
    let row = rig.row_of("paused");
    let column = u16::try_from(rig.gutter_of("paused").chars().count() + 1).unwrap();
    rig.app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));

    assert_eq!(
        rig.app.state().editor().selections().primary().head(),
        before
    );
}
