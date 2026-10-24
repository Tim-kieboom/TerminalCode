//! What the debugger says about the expression under the pointer: the box with the value and its
//! tree, the selection and its errors, and the box that comes again after a stop.

#![cfg(unix)]

use std::time::{Duration, Instant};

use crossterm::event::MouseEventKind;
use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use serde_json::{Value, json};

use super::debug_tests::{debugging, is_stopped, until_the_first_stop};
use super::run_tests::Rig;
use crate::event::action::Action;

/// Longer than the rest the pointer needs on a word.
const LONG_ENOUGH: Duration = Duration::from_millis(450);

/// A rig stopped on the breakpoint of `until_the_first_stop`, with `script` going on from there.
pub(super) fn stopped(
    script: impl FnOnce(&mut crate::dap::test_support::Adapter, &str) + Send + 'static,
) -> Rig {
    let mut rig = debugging(move |adapter, root| {
        until_the_first_stop(adapter, root);
        script(adapter, root);
        while adapter.next().is_some() {}
    });
    rig.open_file("main.rs");
    rig.breakpoint_on(1);
    rig.app.run_action(Action::Debug);
    rig.wait_for("the first stop and its variables", |rig| {
        is_stopped(&rig.app.state().debug().status)
            && rig.app.state().debug().tree.rows().len() == 3
    });
    rig
}

impl Rig {
    /// The pointer rests on the cell `offset` columns into `text` until the debugger is asked.
    pub(super) fn rest_on(&mut self, text: &str, offset: usize) {
        let row = self.row_of(text);
        let column = self.gutter_of(text).chars().count() + offset;
        self.app.handle_input(InputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Moved,
            column: u16::try_from(column).unwrap(),
            row,
            modifiers: KeyModifiers::NONE,
        }));
        std::thread::sleep(LONG_ENOUGH);
        self.app.hover_due();
    }

    pub(super) fn key_with(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
        self.draw();
    }

    /// Lets what the adapter says come in for a while.
    pub(super) fn pump_for(&mut self, time: Duration) {
        let until = Instant::now() + time;
        while Instant::now() < until {
            while let Ok(event) = self.events.try_recv() {
                self.app.handle_event(event);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        self.draw();
    }

    /// The rows of the box as `name = value`.
    pub(super) fn box_rows(&self) -> Vec<String> {
        self.app
            .state()
            .debug_hover()
            .map_or_else(Vec::new, |shown| {
                shown
                    .tree()
                    .rows()
                    .iter()
                    .map(|row| format!("{}{} = {}", "  ".repeat(row.depth), row.name, row.value))
                    .collect()
            })
    }

    pub(super) fn has_box(&self) -> bool {
        self.app.state().debug_hover().is_some()
    }
}

#[test]
fn a_pointer_resting_on_a_name_asks_the_debugger_in_the_frame_and_shows_the_value() {
    let mut rig = stopped(|adapter, _| {
        let (id, arguments) = adapter.expect_request("evaluate");
        assert_eq!(arguments["expression"], "a");
        assert_eq!(arguments["frameId"], 1000);
        assert_eq!(arguments["context"], "hover");
        adapter.respond(
            id,
            "evaluate",
            json!({ "result": "1", "type": "i32", "variablesReference": 0 }),
        );
    });

    rig.rest_on("let a = 1", 4);
    rig.wait_for("the box", |rig| rig.has_box());

    assert_eq!(rig.box_rows(), ["a = 1"]);
}

#[test]
fn a_struct_has_a_tree_that_opens_with_right_once_the_box_is_kept_with_f6() {
    let mut rig = stopped(|adapter, _| {
        let (id, _) = adapter.expect_request("evaluate");
        adapter.respond(
            id,
            "evaluate",
            json!({ "result": "P @ 0x10", "type": "P", "variablesReference": 20 }),
        );
        let (id, arguments) = adapter.expect_request("variables");
        assert_eq!(arguments["variablesReference"], 20);
        adapter.respond(
            id,
            "variables",
            json!({ "variables": [
                { "name": "x", "value": "1", "type": "i32" },
                { "name": "y", "value": "2", "type": "i32" },
            ] }),
        );
    });
    rig.rest_on("let a = 1", 4);
    rig.wait_for("the box", |rig| rig.has_box());
    assert_eq!(rig.box_rows(), ["a = P @ 0x10"]);

    rig.key_with(KeyCode::F(6), KeyModifiers::NONE);
    rig.key_with(KeyCode::Right, KeyModifiers::NONE);

    rig.wait_for("the children", |rig| rig.box_rows().len() == 3);
    assert_eq!(rig.box_rows(), ["a = P @ 0x10", "  x = 1", "  y = 2"]);
    let screen = rig.screen();
    assert!(screen.contains("▾ a = P @ 0x10"), "{screen}");
    assert!(screen.contains("x = 1  i32"), "{screen}");
    // `esc` takes it away and nothing else.
    rig.key_with(KeyCode::Esc, KeyModifiers::NONE);
    assert!(!rig.has_box());
}

#[test]
fn a_selection_that_the_adapter_refuses_shows_what_the_adapter_said() {
    let mut rig = stopped(|adapter, _| {
        let (id, arguments) = adapter.expect_request("evaluate");
        assert_eq!(arguments["expression"], "let a");
        adapter.refuse(
            id,
            "evaluate",
            "<user expression 0>:1:3: unrecognized token",
            Value::Null,
        );
    });
    rig.app.state_mut().edit(|editor| {
        editor.go_to(crate::buffer::Position::new(1, 4));
    });
    for _ in 0..5 {
        rig.key_with(KeyCode::Right, KeyModifiers::SHIFT);
    }

    rig.rest_on("let a = 1", 1);
    rig.wait_for("the box", |rig| rig.has_box());

    let shown = rig.app.state().debug_hover().unwrap();
    assert_eq!(
        shown.error(),
        Some("<user expression 0>:1:3: unrecognized token")
    );
    let screen = rig.screen();
    assert!(screen.contains("unrecognized token"), "{screen}");
}

#[test]
fn a_name_that_the_adapter_refuses_shows_nothing() {
    let (asked, was_asked) = std::sync::mpsc::channel();
    let mut rig = stopped(move |adapter, _| {
        let (id, _) = adapter.expect_request("evaluate");
        adapter.refuse(id, "evaluate", "use of undeclared identifier", Value::Null);
        let _ = asked.send(());
    });

    rig.rest_on("let a = 1", 4);
    rig.wait_for("the answer", |_| was_asked.try_recv().is_ok());
    rig.pump_for(Duration::from_millis(300));

    assert!(!rig.has_box(), "{}", rig.screen());
}

#[test]
fn the_box_asks_again_when_the_program_stops_and_opens_the_rows_that_were_open() {
    let mut rig = stopped(|adapter, root| {
        let (id, _) = adapter.expect_request("evaluate");
        adapter.respond(
            id,
            "evaluate",
            json!({ "result": "P @ 0x10", "variablesReference": 20 }),
        );
        let (id, _) = adapter.expect_request("variables");
        adapter.respond(
            id,
            "variables",
            json!({ "variables": [{ "name": "x", "value": "1" }, { "name": "y", "value": "2" }] }),
        );
        // A step: the program stops again, in another frame.
        let (next, _) = adapter.expect_request("next");
        adapter.respond(next, "next", Value::Null);
        adapter.event("stopped", crate::dap::test_support::stopped("step", 1));
        let (stack, _) = adapter.expect_request("stackTrace");
        adapter.respond(
            stack,
            "stackTrace",
            json!({ "stackFrames": [
                { "id": 2000, "name": "app::main", "source": { "path": format!("{root}/main.rs") }, "line": 3 },
            ] }),
        );
        // The scopes and the box's expression are asked for together.
        let (scopes, _) = adapter.expect_request("scopes");
        let (id, arguments) = adapter.expect_request("evaluate");
        assert_eq!(arguments["expression"], "a");
        assert_eq!(arguments["frameId"], 2000);
        adapter.respond(
            scopes,
            "scopes",
            json!({ "scopes": [{ "name": "Locals", "variablesReference": 9 }] }),
        );
        adapter.respond(
            id,
            "evaluate",
            json!({ "result": "P @ 0x20", "variablesReference": 30 }),
        );
        let (variables, arguments) = adapter.expect_request("variables");
        assert_eq!(arguments["variablesReference"], 9);
        adapter.respond(variables, "variables", json!({ "variables": [] }));
        let (id, arguments) = adapter.expect_request("variables");
        assert_eq!(arguments["variablesReference"], 30);
        adapter.respond(
            id,
            "variables",
            json!({ "variables": [{ "name": "x", "value": "5" }, { "name": "y", "value": "6" }] }),
        );
    });
    rig.rest_on("let a = 1", 4);
    rig.wait_for("the box", |rig| rig.has_box());
    rig.key_with(KeyCode::F(6), KeyModifiers::NONE);
    rig.key_with(KeyCode::Right, KeyModifiers::NONE);
    rig.wait_for("the children", |rig| rig.box_rows().len() == 3);

    rig.key_with(KeyCode::F(10), KeyModifiers::NONE);

    rig.wait_for("the box as the program is now", |rig| {
        rig.box_rows() == ["a = P @ 0x20", "  x = 5", "  y = 6"]
    });
}

#[test]
fn the_box_goes_away_when_the_session_ends() {
    let mut rig = stopped(|adapter, _| {
        let (id, _) = adapter.expect_request("evaluate");
        adapter.respond(id, "evaluate", json!({ "result": "1" }));
        let (bye, _) = adapter.expect_request("disconnect");
        adapter.respond(bye, "disconnect", Value::Null);
    });
    rig.rest_on("let a = 1", 4);
    rig.wait_for("the box", |rig| rig.has_box());

    rig.app.run_action(Action::DebugStop);

    rig.wait_for("the box to go", |rig| !rig.has_box());
}

#[test]
fn a_click_on_the_arrow_of_the_box_opens_it_and_the_button_let_go_does_not_close_it() {
    let mut rig = stopped(|adapter, _| {
        let (id, _) = adapter.expect_request("evaluate");
        adapter.respond(
            id,
            "evaluate",
            json!({ "result": "P @ 0x10", "variablesReference": 20 }),
        );
        let (id, _) = adapter.expect_request("variables");
        adapter.respond(
            id,
            "variables",
            json!({ "variables": [{ "name": "x", "value": "1" }] }),
        );
    });
    rig.rest_on("let a = 1", 4);
    rig.wait_for("the box", |rig| rig.has_box());
    rig.draw();
    let row = rig.row_of("▸ a = P @ 0x10");
    let column = u16::try_from(rig.gutter_of("▸ a = P @ 0x10").chars().count()).unwrap();

    for kind in [
        MouseEventKind::Down(crossterm::event::MouseButton::Left),
        MouseEventKind::Up(crossterm::event::MouseButton::Left),
    ] {
        rig.app.handle_input(InputEvent::Mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    }

    rig.wait_for("the children", |rig| rig.box_rows().len() == 2);
    assert!(rig.has_box());
    assert_eq!(rig.box_rows(), ["a = P @ 0x10", "  x = 1"]);
}
