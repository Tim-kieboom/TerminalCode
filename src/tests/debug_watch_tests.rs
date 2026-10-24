//! The watches: pinning a variable from what is on the screen, what a watch comes to at each
//! stop, and what is remembered.

use crossterm::event::{KeyCode, KeyModifiers};
use serde_json::{Value, json};

use super::debug_hover_tests::stopped;
use super::debug_pane_tests::chord;
use super::run_tests::Rig;
use crate::app::watches_for_tests::path_expression;
use crate::components::debug::Section;
use crate::event::action::Action;

impl Rig {
    /// The watches as `name = value` (children indented).
    pub(super) fn watch_rows(&self) -> Vec<String> {
        self.app
            .state()
            .debug()
            .watches
            .rows()
            .iter()
            .map(|row| format!("{}{} = {}", "  ".repeat(row.depth), row.name, row.value))
            .collect()
    }

    /// The Debug pane has the keyboard, on the variable `a` (the second row).
    fn on_the_variable_a(&mut self) {
        chord(self, 'k', 'v');
        self.press(KeyCode::Tab);
        self.press(KeyCode::Down);
        assert_eq!(self.app.state().debug_view().section(), Section::Variables);
    }

    fn state_file(&self) -> String {
        std::fs::read_to_string(self.config.path().join("state.json")).unwrap_or_default()
    }
}

#[test]
fn w_pins_the_variable_as_a_watch_that_is_evaluated_and_remembered() {
    let mut rig = stopped(|adapter, _| {
        let (id, arguments) = adapter.expect_request("evaluate");
        assert_eq!(arguments["expression"], "a");
        assert_eq!(arguments["frameId"], 1000);
        assert_eq!(arguments["context"], "hover");
        adapter.respond(id, "evaluate", json!({ "result": "1", "type": "i32" }));
    });
    rig.on_the_variable_a();

    rig.press(KeyCode::Char('w'));

    rig.wait_for("the value of the watch", |rig| {
        rig.watch_rows() == ["a = 1"]
    });
    assert!(
        rig.state_file().contains("\"watches\""),
        "{}",
        rig.state_file()
    );
    let screen = rig.screen();
    assert!(screen.contains("a = 1"), "{screen}");
}

#[test]
fn a_watch_is_asked_for_again_at_each_stop_and_the_rows_that_were_open_are_opened_again() {
    let mut rig = stopped(|adapter, root| {
        let (id, _) = adapter.expect_request("evaluate");
        adapter.respond(
            id,
            "evaluate",
            json!({ "result": "P @ 0x10", "variablesReference": 20 }),
        );
        let (id, arguments) = adapter.expect_request("variables");
        assert_eq!(arguments["variablesReference"], 20);
        adapter.respond(
            id,
            "variables",
            json!({ "variables": [{ "name": "x", "value": "1" }, { "name": "y", "value": "2" }] }),
        );
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
    rig.on_the_variable_a();
    rig.press(KeyCode::Char('w'));
    rig.wait_for("the watch", |rig| rig.watch_rows() == ["a = P @ 0x10"]);
    // Opened in its own list.
    rig.press(KeyCode::Tab);
    rig.press(KeyCode::Right);
    rig.wait_for("its children", |rig| rig.watch_rows().len() == 3);

    rig.app.run_action(Action::DebugStepOver);

    rig.wait_for("the watch as the program is now", |rig| {
        rig.watch_rows() == ["a = P @ 0x20", "  x = 5", "  y = 6"]
    });
}

#[test]
fn a_watch_the_adapter_refuses_shows_its_words_in_the_row() {
    let mut rig = stopped(|adapter, _| {
        let (id, _) = adapter.expect_request("evaluate");
        adapter.refuse(
            id,
            "evaluate",
            "use of undeclared identifier 'a'\n   1 | a\n     | ^",
            Value::Null,
        );
    });
    rig.on_the_variable_a();

    rig.press(KeyCode::Char('w'));

    rig.wait_for("the row", |rig| {
        rig.watch_rows() == ["a = use of undeclared identifier 'a'"]
    });
}

#[test]
fn delete_takes_a_watch_away_and_a_watch_outlives_the_session_without_its_value() {
    let mut rig = stopped(|adapter, _| {
        let (id, _) = adapter.expect_request("evaluate");
        adapter.respond(id, "evaluate", json!({ "result": "1" }));
        let (bye, _) = adapter.expect_request("disconnect");
        adapter.respond(bye, "disconnect", Value::Null);
    });
    rig.on_the_variable_a();
    rig.press(KeyCode::Char('w'));
    rig.wait_for("the watch", |rig| rig.watch_rows() == ["a = 1"]);

    rig.app.run_action(Action::DebugStop);
    rig.wait_for("the end", |rig| {
        matches!(
            rig.app.state().debug().status,
            crate::components::debug::model::Status::Ended(_)
        )
    });

    assert_eq!(
        rig.watch_rows(),
        ["a = "],
        "the expression stays, its value does not"
    );
    // Pinned again is told as it is: it is there.
    rig.on_the_watches();
    rig.press(KeyCode::Delete);
    assert!(rig.watch_rows().is_empty());
    assert!(
        !rig.state_file().contains("\"watches\""),
        "{}",
        rig.state_file()
    );
}

impl Rig {
    fn on_the_watches(&mut self) {
        while self.app.state().debug_view().section() != Section::Watches {
            self.press(KeyCode::Tab);
        }
        assert_eq!(self.app.state().debug_view().section(), Section::Watches);
    }
}

#[test]
fn the_path_of_a_row_is_the_expression_of_its_watch() {
    let path = |names: &[&str]| -> Vec<String> { names.iter().map(|s| (*s).to_owned()).collect() };

    assert_eq!(path_expression(&path(&["a"])), "a");
    assert_eq!(path_expression(&path(&["a", "name", "len"])), "a.name.len");
    assert_eq!(path_expression(&path(&["v", "[0]", "x"])), "v[0].x");
}

#[test]
fn w_in_the_hover_box_pins_the_row_the_selection_is_on() {
    let mut rig = stopped(|adapter, _| {
        let (id, arguments) = adapter.expect_request("evaluate");
        assert_eq!(arguments["expression"], "a");
        adapter.respond(id, "evaluate", json!({ "result": "1" }));
        let (id, arguments) = adapter.expect_request("evaluate");
        assert_eq!(arguments["expression"], "a");
        adapter.respond(id, "evaluate", json!({ "result": "1" }));
    });
    rig.rest_on("let a = 1", 4);
    rig.wait_for("the box", |rig| rig.has_box());
    rig.key_with(KeyCode::F(6), KeyModifiers::NONE);

    rig.press(KeyCode::Char('w'));

    rig.wait_for("the watch", |rig| rig.watch_rows() == ["a = 1"]);
    assert!(rig.has_box(), "the box stays");
}
