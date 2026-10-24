//! The debug session, from the key to a stop and back, against a scripted adapter. The program is
//! built by a stand-in cargo and is a script, as in `run_tests`.

#![cfg(unix)]

use std::fs;
use std::sync::{Arc, Mutex};

use crossterm::event::KeyCode;
use serde_json::{Value, json};

use super::run_tests::{Rig, Setup, rig_with};
use crate::app::debug::Adapter as AdapterFactory;
use crate::breakpoints::Kind;
use crate::components::debug::model::Status;
use crate::dap::DapError;
use crate::dap::test_support::{Adapter, fake_adapter_waking, stopped};
use crate::event::Event;
use crate::event::action::Action;

const SOURCE: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n}\n";

/// An adapter factory that starts the scripted adapter, once.
pub(super) fn scripted(script: impl FnOnce(&mut Adapter) + Send + 'static) -> AdapterFactory {
    let script = Arc::new(Mutex::new(Some(script)));
    Arc::new(move |wake| {
        let script = script
            .lock()
            .unwrap()
            .take()
            .expect("the adapter is started once");
        let (client, fake) = fake_adapter_waking(wake, script);
        // The script ends with the test; nobody waits for it here.
        std::mem::forget(fake);
        Ok(client)
    })
}

/// A rig with `main.rs` open and a breakpoint on its second line, which `script` plays the
/// adapter for.
pub(super) fn debugging(script: impl FnOnce(&mut Adapter, &str) + Send + 'static) -> Rig {
    let setup = Setup::default();
    let mut rig = rig_with(setup, None);
    let root = rig.root.display().to_string();
    fs::write(rig.root.join("main.rs"), SOURCE).unwrap();
    let factory = scripted(move |adapter| script(adapter, &root));
    rig.app = std::mem::replace(
        &mut rig.app,
        crate::app::App::new(crate::app::state::AppState::default()),
    )
    .with_debug_adapter(factory);
    rig
}

impl Rig {
    pub(super) fn open_file(&mut self, name: &str) {
        let path = self.root.join(name);
        self.app
            .state_mut()
            .workspace_mut()
            .open_path(&path)
            .unwrap();
        self.app.handle_event(Event::TerminalChanged);
        self.draw();
    }

    pub(super) fn breakpoint_on(&mut self, line: usize) {
        self.app.state_mut().edit(|editor| {
            editor.go_to(crate::buffer::Position::new(line, 0));
        });
        // The key, as the user presses it: the hooks after an input hand the adapter what changed.
        self.app.handle_input(crossterm::event::Event::Key(
            crossterm::event::KeyEvent::new(KeyCode::F(9), crossterm::event::KeyModifiers::NONE),
        ));
    }

    /// The color behind the cell at `column` of `row`.
    pub(super) fn background_at(&self, column: u16, row: u16) -> ratatui::style::Color {
        self.terminal.backend().buffer()[(column, row)].bg
    }

    /// The text of every row of the screen, a cell for each column.
    fn cells(&self) -> Vec<String> {
        let buffer = self.terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_owned())
                    .collect()
            })
            .collect()
    }

    /// The screen row that shows `text`.
    pub(super) fn row_of(&self, text: &str) -> u16 {
        self.cells()
            .iter()
            .position(|row| row.contains(text))
            .map(|row| row as u16)
            .unwrap_or_else(|| panic!("{text} is not on the screen"))
    }

    /// The columns before `text` on its row.
    pub(super) fn gutter_of(&self, text: &str) -> String {
        let cells = self.cells();
        let row = cells
            .iter()
            .find(|row| row.contains(text))
            .unwrap_or_else(|| panic!("{text} is not on the screen"));
        row[..row.find(text).unwrap()].to_owned()
    }

    pub(super) fn status(&self) -> Status {
        self.app.state().debug().status.clone()
    }

    pub(super) fn wait_for_status(&mut self, what: &str, want: impl Fn(&Status) -> bool) {
        self.wait_for(what, |rig| want(&rig.app.state().debug().status));
    }
}

pub(super) fn is_stopped(status: &Status) -> bool {
    matches!(status, Status::Stopped { .. })
}

/// What an adapter says to take a session from the start to the first stop on the breakpoint at
/// line 2 of `main.rs` (from 1): asserts what the client sends, in the order it does.
fn until_the_first_stop(adapter: &mut Adapter, root: &str) {
    let arguments = adapter.answer_initialize(json!({ "supportsConfigurationDoneRequest": true }));
    assert_eq!(arguments["adapterID"], "lldb-dap");
    assert_eq!(arguments["supportsRunInTerminalRequest"], true);
    let (launch, launching) = adapter.expect_request("launch");
    assert!(
        launching["program"]
            .as_str()
            .unwrap()
            .ends_with("program.sh")
    );
    assert_eq!(launching["runInTerminal"], true);
    assert_eq!(launching["stopOnEntry"], false);
    assert_eq!(launching["cwd"], root);
    adapter.event("initialized", Value::Null);

    let (set, arguments) = adapter.expect_request("setBreakpoints");
    assert_eq!(arguments["source"]["path"], format!("{root}/main.rs"));
    assert_eq!(arguments["breakpoints"], json!([{ "line": 2 }]));
    adapter.respond(
        set,
        "setBreakpoints",
        json!({ "breakpoints": [{ "id": 1, "verified": true, "line": 2 }] }),
    );
    let (done, _) = adapter.expect_request("configurationDone");
    adapter.respond(done, "configurationDone", Value::Null);
    adapter.respond(launch, "launch", Value::Null);

    // The program is started in the tab, which the editor does.
    let run = adapter.reverse_request(
        "runInTerminal",
        json!({ "cwd": root, "args": [format!("{root}/program.sh")] }),
    );
    let (answered, outcome) = adapter.expect_response();
    assert_eq!(answered, run);
    assert!(outcome.unwrap()["processId"].as_i64().unwrap() > 0);

    adapter.event("stopped", stopped("breakpoint", 1));
    let (stack, arguments) = adapter.expect_request("stackTrace");
    assert_eq!(arguments["threadId"], 1);
    adapter.respond(
        stack,
        "stackTrace",
        json!({ "stackFrames": [
            { "id": 1000, "name": "app::main", "source": { "path": format!("{root}/main.rs") },
              "line": 2, "column": 5 },
            { "id": 1001, "name": "start" },
        ] }),
    );
    let (scopes, arguments) = adapter.expect_request("scopes");
    assert_eq!(arguments["frameId"], 1000);
    adapter.respond(
        scopes,
        "scopes",
        json!({ "scopes": [{ "name": "Locals", "variablesReference": 7 }, { "name": "Registers", "variablesReference": 8 }] }),
    );
    let (variables, arguments) = adapter.expect_request("variables");
    assert_eq!(arguments["variablesReference"], 7);
    adapter.respond(
        variables,
        "variables",
        json!({ "variables": [{ "name": "a", "value": "1", "type": "i32" }] }),
    );
}

#[test]
fn debug_builds_starts_the_adapter_and_stops_on_a_breakpoint_with_the_stack_and_the_variables() {
    let mut rig = debugging(|adapter, root| {
        until_the_first_stop(adapter, root);
        // Hold on until the test is done.
        while adapter.next().is_some() {}
    });
    // Wide, so that the messages (at the right) do not cover the pane.
    rig.terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(220, 30)).unwrap();
    rig.open_file("main.rs");
    rig.breakpoint_on(1);

    rig.app.run_action(Action::Debug);
    rig.wait_for_status("a stop", is_stopped);

    // The program was started in the tab by the editor, because the adapter asked.
    rig.wait_for("the program's output", |rig| {
        rig.tab_text().contains("hello from the program")
    });
    assert_eq!(rig.tabs(), 1);
    let data = rig.app.state().debug();
    assert_eq!(
        data.status,
        Status::Stopped {
            reason: "breakpoint".into(),
            description: None
        }
    );
    rig.wait_for("the stack and the variables", |rig| {
        let data = rig.app.state().debug();
        data.frames.len() == 2 && data.tree.rows().len() == 3
    });
    let data = rig.app.state().debug();
    assert_eq!(data.frames[0].name, "app::main");
    assert_eq!(data.frames[0].line, 1, "from 0");
    assert!(data.frames[1].path.is_none(), "a frame with no file");
    let rows: Vec<String> = data
        .tree
        .rows()
        .iter()
        .map(|row| format!("{}{}", row.name, row.value))
        .collect();
    assert_eq!(rows, ["Locals", "a1", "Registers"]);
    assert!(data.tree.rows()[0].expanded, "the locals are open");
    // The file of the stop is in front, with the cursor on the line.
    let cursor = rig.app.state().editor().selections().primary().head();
    assert_eq!(cursor.line, 1);
    assert!(
        rig.said_has("stopped (breakpoint) in app::main main.rs:2"),
        "{:?}",
        rig.said()
    );
    // The Debug pane came to the front with the stack and the variables.
    rig.draw();
    let screen = rig.screen();
    assert!(screen.contains("stopped (breakpoint)"), "{screen}");
    assert!(screen.contains("▶ app::main  main.rs:2"), "{screen}");
    assert!(screen.contains("▾ Locals"), "{screen}");
    assert!(screen.contains("a = 1  i32"), "{screen}");
    // The line the program is stopped on has the arrow in the gutter and a color behind it.
    rig.draw();
    assert!(
        rig.gutter_of("    let a = 1;").contains('▶'),
        "{}",
        rig.screen()
    );
    assert!(!rig.gutter_of("    let b = 2;").contains('▶'));
    let (stopped_row, other_row) = (rig.row_of("let a = 1;"), rig.row_of("let b = 2;"));
    let column = rig.gutter_of("    let a = 1;").chars().count() as u16 + 6;
    assert_ne!(
        rig.background_at(column, stopped_row),
        rig.background_at(column, other_row)
    );
    // The adapter verified the breakpoint.
    let shown = rig
        .app
        .state()
        .breakpoints()
        .shown(rig.app.state().workspace().active_document_id().unwrap());
    assert_eq!(shown[0].kind, Kind::Set);
}

#[test]
fn a_breakpoint_the_adapter_cannot_place_goes_hollow() {
    let mut rig = debugging(|adapter, root| {
        adapter.answer_initialize(json!({}));
        let (launch, _) = adapter.expect_request("launch");
        adapter.event("initialized", Value::Null);
        let (set, _) = adapter.expect_request("setBreakpoints");
        adapter.respond(
            set,
            "setBreakpoints",
            json!({ "breakpoints": [{ "verified": false, "message": "no code at that line" }] }),
        );
        let (done, _) = adapter.expect_request("configurationDone");
        adapter.respond(done, "configurationDone", Value::Null);
        adapter.respond(launch, "launch", Value::Null);
        let _ = root;
        while adapter.next().is_some() {}
    });
    rig.open_file("main.rs");
    rig.breakpoint_on(1);

    rig.app.run_action(Action::Debug);

    rig.wait_for("the breakpoint to go hollow", |rig| {
        let document = rig.app.state().workspace().active_document_id().unwrap();
        rig.app
            .state()
            .breakpoints()
            .shown(document)
            .first()
            .map(|s| s.kind)
            == Some(Kind::Hollow)
    });
    rig.draw();
    assert!(rig.screen().contains('○'), "{}", rig.screen());
}

#[test]
fn stepping_goes_through_the_adapter_and_the_next_stop_replaces_the_stack() {
    let mut rig = debugging(|adapter, root| {
        until_the_first_stop(adapter, root);
        let (next, arguments) = adapter.expect_request("next");
        assert_eq!(arguments["threadId"], 1);
        adapter.respond(next, "next", Value::Null);
        adapter.event("stopped", stopped("step", 1));
        let (stack, _) = adapter.expect_request("stackTrace");
        adapter.respond(stack, "stackTrace", json!({ "stackFrames": [
            { "id": 2000, "name": "app::main", "source": { "path": format!("{root}/main.rs") }, "line": 3 },
        ] }));
        let (scopes, _) = adapter.expect_request("scopes");
        adapter.respond(
            scopes,
            "scopes",
            json!({ "scopes": [{ "name": "Locals", "variablesReference": 9 }] }),
        );
        let (variables, _) = adapter.expect_request("variables");
        adapter.respond(
            variables,
            "variables",
            json!({ "variables": [
            { "name": "a", "value": "1" }, { "name": "b", "value": "2" },
        ] }),
        );
        while adapter.next().is_some() {}
    });
    rig.open_file("main.rs");
    rig.breakpoint_on(1);
    rig.app.run_action(Action::Debug);
    rig.wait_for("the first stop and its variables", |rig| {
        is_stopped(&rig.app.state().debug().status)
            && rig.app.state().debug().tree.rows().len() == 3
    });

    rig.app.run_action(Action::DebugStepOver);
    assert_eq!(
        rig.status(),
        Status::Running,
        "until the adapter says where it stopped"
    );
    rig.wait_for("the second stop", |rig| {
        rig.app.state().debug().frames.first().map(|f| f.id) == Some(2000)
            && rig.app.state().debug().tree.rows().len() == 3
    });

    let rows: Vec<String> = rig
        .app
        .state()
        .debug()
        .tree
        .rows()
        .iter()
        .map(|r| r.name.clone())
        .collect();
    assert_eq!(rows, ["Locals", "a", "b"]);
    assert_eq!(rig.app.state().debug().frames[0].line, 2);
}

#[test]
fn f5_goes_on_while_the_program_is_stopped_and_the_end_of_the_program_ends_the_session() {
    let mut rig = debugging(|adapter, root| {
        until_the_first_stop(adapter, root);
        let (go, arguments) = adapter.expect_request("continue");
        assert_eq!(arguments["threadId"], 1);
        // The program ends before the answer: an event may come first.
        adapter.event("exited", json!({ "exitCode": 0 }));
        adapter.event("terminated", Value::Null);
        adapter.respond(go, "continue", json!({ "allThreadsContinued": true }));
        while adapter.next().is_some() {}
    });
    rig.open_file("main.rs");
    rig.breakpoint_on(1);
    rig.app.run_action(Action::Debug);
    rig.wait_for_status("a stop", is_stopped);
    rig.wait_for("the variables", |rig| {
        rig.app.state().debug().tree.rows().len() == 3
    });

    // The key that runs a program goes on while one is stopped.
    rig.app.run_action(Action::Run);

    rig.wait_for_status("the end", |status| matches!(status, Status::Ended(_)));
    assert!(rig.said_has("the program ended"), "{:?}", rig.said());
    assert!(rig.app.state().debug().frames.is_empty());
    assert_eq!(
        rig.tabs(),
        1,
        "the tab of the program stays, as a run's does"
    );
}

#[test]
fn stop_ends_the_session_and_asks_the_adapter_to_end_the_program() {
    let mut rig = debugging(|adapter, root| {
        until_the_first_stop(adapter, root);
        let (bye, arguments) = adapter.expect_request("disconnect");
        assert_eq!(arguments["terminateDebuggee"], true);
        adapter.respond(bye, "disconnect", Value::Null);
        while adapter.next().is_some() {}
    });
    rig.open_file("main.rs");
    rig.breakpoint_on(1);
    rig.app.run_action(Action::Debug);
    rig.wait_for_status("a stop", is_stopped);
    rig.wait_for("the variables", |rig| {
        rig.app.state().debug().tree.rows().len() == 3
    });

    rig.app.run_action(Action::DebugStop);

    rig.wait_for_status("the end", |status| matches!(status, Status::Ended(_)));
    assert!(
        rig.said_has("the debug session was stopped"),
        "{:?}",
        rig.said()
    );
}

#[test]
fn a_second_debug_while_one_goes_says_so_and_a_step_with_no_session_says_so() {
    let mut rig = debugging(|adapter, root| {
        until_the_first_stop(adapter, root);
        while adapter.next().is_some() {}
    });
    rig.app.run_action(Action::DebugStepOver);
    assert!(rig.said_has("there is no debug session"));
    rig.open_file("main.rs");
    rig.breakpoint_on(1);
    rig.app.run_action(Action::Debug);
    rig.wait_for_status("a stop", is_stopped);

    rig.app.run_action(Action::Debug);

    assert!(rig.said_has("a debug session is going"), "{:?}", rig.said());
}

#[test]
fn a_step_while_the_program_runs_says_so() {
    let mut rig = debugging(|adapter, root| {
        adapter.answer_initialize(json!({}));
        let (launch, _) = adapter.expect_request("launch");
        adapter.event("initialized", Value::Null);
        let (done, _) = adapter.expect_request("configurationDone");
        adapter.respond(done, "configurationDone", Value::Null);
        adapter.respond(launch, "launch", Value::Null);
        let _ = root;
        while adapter.next().is_some() {}
    });
    rig.app.run_action(Action::Debug);
    rig.wait_for_status("the program to run", |status| *status == Status::Running);

    rig.app.run_action(Action::DebugStepInto);

    assert!(rig.said_has("the program is running"), "{:?}", rig.said());
}

#[test]
fn a_breakpoint_set_during_the_session_is_handed_over() {
    let mut rig = debugging(|adapter, root| {
        until_the_first_stop(adapter, root);
        let (set, arguments) = adapter.expect_request("setBreakpoints");
        assert_eq!(
            arguments["breakpoints"],
            json!([{ "line": 2 }, { "line": 3 }])
        );
        adapter.respond(
            set,
            "setBreakpoints",
            json!({ "breakpoints": [
            { "verified": true }, { "verified": false },
        ] }),
        );
        let _ = root;
        while adapter.next().is_some() {}
    });
    rig.open_file("main.rs");
    rig.breakpoint_on(1);
    rig.app.run_action(Action::Debug);
    rig.wait_for_status("a stop", is_stopped);
    rig.wait_for("the variables", |rig| {
        rig.app.state().debug().tree.rows().len() == 3
    });

    rig.breakpoint_on(2);

    rig.wait_for("the new breakpoint to be answered", |rig| {
        let document = rig.app.state().workspace().active_document_id().unwrap();
        rig.app
            .state()
            .breakpoints()
            .shown(document)
            .iter()
            .map(|s| s.kind)
            .collect::<Vec<_>>()
            == [Kind::Set, Kind::Hollow]
    });
}

#[test]
fn an_adapter_that_is_not_installed_says_how_to_get_it_and_the_tab_says_so_too() {
    let adapter: AdapterFactory = Arc::new(|_wake| {
        Err(DapError::Spawn {
            program: "lldb-dap".to_owned(),
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        })
    });
    let mut rig = rig_with(Setup::default(), Some(adapter));

    rig.app.run_action(Action::Debug);

    rig.wait_for("the message", |rig| rig.said_has("is not installed"));
    rig.wait_for("the tab to say so", |rig| {
        rig.tab_text()
            .contains("[the debug adapter `lldb-dap` is not installed")
    });
    assert!(rig.said_has("comes with LLVM"), "{:?}", rig.said());
    assert!(!rig.app.state().debug().status.is_active());
}

#[test]
fn a_launch_the_adapter_refuses_ends_the_session_with_its_words() {
    let mut rig = debugging(|adapter, root| {
        adapter.answer_initialize(json!({}));
        let (launch, _) = adapter.expect_request("launch");
        adapter.refuse(
            launch,
            "launch",
            "failed",
            json!({ "error": { "id": 1, "format": "program does not exist" } }),
        );
        let _ = root;
        while adapter.next().is_some() {}
    });

    rig.app.run_action(Action::Debug);

    rig.wait_for_status("the end", |status| matches!(status, Status::Ended(_)));
    assert!(
        rig.said_has(
            "could not be launched: the debug adapter refused launch: program does not exist"
        ),
        "{:?}",
        rig.said()
    );
    rig.wait_for("the tab to say so", |rig| {
        rig.tab_text()
            .contains("[the program could not be launched")
    });
}

#[test]
fn an_adapter_that_goes_away_ends_the_session() {
    let mut rig = debugging(|adapter, _root| {
        adapter.answer_initialize(json!({}));
        // Leaves.
    });

    rig.app.run_action(Action::Debug);

    rig.wait_for_status("the end", |status| matches!(status, Status::Ended(_)));
    assert!(
        rig.said_has("the debug adapter went away"),
        "{:?}",
        rig.said()
    );
}

#[test]
fn a_command_of_the_file_cannot_be_debugged() {
    let mut rig = rig_with(
        Setup {
            has_target: false,
            launch_file: Some("[[run]]\nname = \"Hi\"\ncommand = \"echo\"\n"),
            ..Setup::default()
        },
        None,
    );

    rig.app.run_action(Action::Debug);

    rig.wait_for("the message", |rig| {
        rig.said_has("only a binary or an example")
    });
    assert_eq!(rig.tabs(), 0);
}

#[test]
fn a_build_that_fails_starts_no_adapter() {
    let adapter: AdapterFactory = Arc::new(|_wake| panic!("no adapter is wanted"));
    let mut rig = rig_with(
        Setup {
            build_code: 101,
            build_says: "error: broken",
            ..Setup::default()
        },
        Some(adapter),
    );

    rig.app.run_action(Action::Debug);

    rig.wait_for_text("[the build failed");
}

#[test]
fn the_palette_and_the_chords_have_the_debug_commands() {
    let titles: Vec<&str> = Action::palette_actions()
        .iter()
        .filter_map(Action::title)
        .collect();
    for wanted in [
        "Debug: Start",
        "Debug: Continue",
        "Debug: Step Over",
        "Debug: Step Into",
        "Debug: Step Out",
        "Debug: Stop",
    ] {
        assert!(titles.contains(&wanted), "{wanted}");
    }
}
