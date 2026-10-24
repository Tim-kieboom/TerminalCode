//! The Debug pane: its keys, its mouse, the frames it chooses and the variables it opens, against
//! a scripted adapter (see `debug_tests`).

#![cfg(unix)]

use crossterm::event::{
    Event as InputEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use serde_json::{Value, json};

use super::debug_tests::{debugging, is_stopped, until_the_first_stop};
use super::run_tests::Rig;
use crate::app::state::Focus;
use crate::components::debug::Section;
use crate::dap::test_support::{Adapter, stopped};
use crate::event::action::Action;

/// A stop with two frames (the second in `other.rs`) and, in the locals, a point with children
/// and a list of 250.
fn until_a_stop_with_things_to_open(adapter: &mut Adapter, root: &str) {
    adapter.answer_initialize(json!({}));
    let (launch, _) = adapter.expect_request("launch");
    adapter.event("initialized", Value::Null);
    let (set, _) = adapter.expect_request("setBreakpoints");
    adapter.respond(
        set,
        "setBreakpoints",
        json!({ "breakpoints": [{ "verified": true }] }),
    );
    let (done, _) = adapter.expect_request("configurationDone");
    adapter.respond(done, "configurationDone", Value::Null);
    adapter.respond(launch, "launch", Value::Null);
    adapter.event("stopped", stopped("breakpoint", 1));
    let (stack, _) = adapter.expect_request("stackTrace");
    adapter.respond(
        stack,
        "stackTrace",
        json!({ "stackFrames": [
            { "id": 1000, "name": "app::main", "source": { "path": format!("{root}/main.rs") }, "line": 2 },
            { "id": 1001, "name": "caller", "source": { "path": format!("{root}/other.rs") }, "line": 1 },
        ] }),
    );
    let (scopes, _) = adapter.expect_request("scopes");
    adapter.respond(
        scopes,
        "scopes",
        json!({ "scopes": [{ "name": "Locals", "variablesReference": 7 }] }),
    );
    let (variables, _) = adapter.expect_request("variables");
    adapter.respond(
        variables,
        "variables",
        json!({ "variables": [
            { "name": "point", "value": "Point", "variablesReference": 9 },
            { "name": "items", "value": "Vec(250)", "variablesReference": 11, "indexedVariables": 250 },
            { "name": "n", "value": "3", "type": "i32" },
        ] }),
    );
}

fn at_a_stop(script_after: impl FnOnce(&mut Adapter, &str) + Send + 'static) -> Rig {
    let mut rig = debugging(move |adapter, root| {
        until_a_stop_with_things_to_open(adapter, root);
        script_after(adapter, root);
        while adapter.next().is_some() {}
    });
    std::fs::write(rig.root.join("other.rs"), "fn other() {}\n").unwrap();
    rig.terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(220, 30)).unwrap();
    rig.open_file("main.rs");
    rig.breakpoint_on(1);
    rig.app.run_action(Action::Debug);
    rig.wait_for_status("a stop", is_stopped);
    rig.wait_for("the variables", |rig| {
        rig.app.state().debug().tree.rows().len() == 4
    });
    rig
}

pub(super) fn chord(rig: &mut Rig, first: char, second: char) {
    rig.app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char(first),
        KeyModifiers::CONTROL,
    )));
    rig.app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::Char(second),
        KeyModifiers::NONE,
    )));
}

fn rows(rig: &Rig) -> Vec<String> {
    rig.app
        .state()
        .debug()
        .tree
        .rows()
        .iter()
        .map(|row| format!("{}{}", "  ".repeat(row.depth), row.name))
        .collect()
}

fn click(rig: &mut Rig, column: u16, row: u16) {
    rig.app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

#[test]
fn the_pane_is_shown_with_the_keyboard_by_a_chord_and_hidden_by_the_same_chord() {
    let mut rig = at_a_stop(|_, _| {});
    assert_eq!(
        rig.app.state().focus(),
        Focus::Editor,
        "a stop gives the keyboard to the editor"
    );

    chord(&mut rig, 'k', 'v');
    assert_eq!(rig.app.state().focus(), Focus::Debug);

    chord(&mut rig, 'k', 'v');
    assert_eq!(rig.app.state().focus(), Focus::Editor);
    rig.draw();
    assert!(!rig.screen().contains("▾ Locals"), "the pane is hidden");
}

#[test]
fn tab_switches_lists_and_enter_on_a_frame_chooses_it_shows_its_file_and_reads_its_variables() {
    let mut rig = at_a_stop(|adapter, _| {
        let (scopes, arguments) = adapter.expect_request("scopes");
        assert_eq!(arguments["frameId"], 1001);
        adapter.respond(
            scopes,
            "scopes",
            json!({ "scopes": [{ "name": "Locals", "variablesReference": 21 }] }),
        );
        let (variables, _) = adapter.expect_request("variables");
        adapter.respond(
            variables,
            "variables",
            json!({ "variables": [{ "name": "inner", "value": "5" }] }),
        );
    });
    chord(&mut rig, 'k', 'v');
    assert_eq!(rig.app.state().debug_view().section(), Section::Stack);

    rig.press(KeyCode::Down);
    rig.press(KeyCode::Enter);

    assert_eq!(rig.app.state().debug().selected_frame, 1);
    rig.wait_for("the variables of the frame", |rig| {
        rows(rig) == ["Locals", "  inner"]
    });
    // The file of the frame is in front, the keyboard stays in the pane.
    let name = rig
        .app
        .state()
        .editor()
        .buffer()
        .path()
        .unwrap()
        .file_name()
        .unwrap()
        .to_owned();
    assert_eq!(name, "other.rs");
    assert_eq!(rig.app.state().focus(), Focus::Debug);
    // The arrow of the stack is on the chosen frame.
    rig.draw();
    assert!(rig.screen().contains("▶ caller"), "{}", rig.screen());

    rig.press(KeyCode::Tab);
    assert_eq!(rig.app.state().debug_view().section(), Section::Variables);
    rig.press(KeyCode::Tab);
    assert_eq!(rig.app.state().debug_view().section(), Section::Watches);
    rig.press(KeyCode::Tab);
    assert_eq!(rig.app.state().debug_view().section(), Section::Stack);
}

#[test]
fn right_opens_a_variable_and_left_closes_it() {
    let mut rig = at_a_stop(|adapter, _| {
        let (variables, arguments) = adapter.expect_request("variables");
        assert_eq!(arguments["variablesReference"], 9);
        assert!(
            arguments.get("start").is_none(),
            "a short list is read whole"
        );
        adapter.respond(
            variables,
            "variables",
            json!({ "variables": [
            { "name": "x", "value": "1" }, { "name": "y", "value": "2" },
        ] }),
        );
    });
    chord(&mut rig, 'k', 'v');
    rig.press(KeyCode::Tab);
    rig.press(KeyCode::Down);
    rig.press(KeyCode::Down);
    assert_eq!(rig.app.state().debug_view().selected_variable(), 2);
    rig.press(KeyCode::Up);
    assert_eq!(rig.app.state().debug_view().selected_variable(), 1);

    rig.press(KeyCode::Right);
    rig.wait_for("the children", |rig| rows(rig).len() == 6);
    assert_eq!(
        rows(&rig),
        ["Locals", "  point", "    x", "    y", "  items", "  n"]
    );
    rig.draw();
    assert!(rig.screen().contains("▾ point"), "{}", rig.screen());

    rig.press(KeyCode::Left);
    assert_eq!(rows(&rig), ["Locals", "  point", "  items", "  n"]);
    // Right on one that is open already, and left on one that is closed, do nothing.
    rig.press(KeyCode::Left);
    assert_eq!(rows(&rig).len(), 4);
}

#[test]
fn a_long_list_is_read_a_page_at_a_time() {
    let mut rig = at_a_stop(|adapter, _| {
        let (first, arguments) = adapter.expect_request("variables");
        assert_eq!(
            (
                &arguments["variablesReference"],
                &arguments["start"],
                &arguments["count"]
            ),
            (&json!(11), &json!(0), &json!(100))
        );
        let page: Vec<Value> = (0..100)
            .map(|n| json!({ "name": format!("[{n}]"), "value": "0" }))
            .collect();
        adapter.respond(first, "variables", json!({ "variables": page }));
        let (second, arguments) = adapter.expect_request("variables");
        assert_eq!(
            (&arguments["start"], &arguments["count"]),
            (&json!(100), &json!(100))
        );
        let page: Vec<Value> = (100..200)
            .map(|n| json!({ "name": format!("[{n}]"), "value": "0" }))
            .collect();
        adapter.respond(second, "variables", json!({ "variables": page }));
    });
    chord(&mut rig, 'k', 'v');
    rig.press(KeyCode::Tab);
    rig.press(KeyCode::Down);
    rig.press(KeyCode::Down);
    rig.press(KeyCode::Enter);
    rig.wait_for("the first page", |rig| rows(rig).len() == 4 + 100 + 1);
    let more = rig
        .app
        .state()
        .debug()
        .tree
        .rows()
        .iter()
        .position(|row| row.value.starts_with("… "))
        .unwrap();
    assert_eq!(
        rig.app.state().debug().tree.rows()[more].value,
        "… 150 more"
    );

    // The row that says how much is left reads the next page when it is opened.
    rig.app.state_mut().debug_view_mut().apply(
        crate::components::debug::DebugCommand::Last,
        crate::components::debug::Counts::default(),
    );
    for _ in 0..more {
        rig.press(KeyCode::Down);
    }
    rig.press(KeyCode::Enter);

    rig.wait_for("the second page", |rig| {
        rig.app.state().debug().tree.rows().len() == 4 + 200 + 1
    });
    let last = rig
        .app
        .state()
        .debug()
        .tree
        .rows()
        .iter()
        .find(|row| row.value.starts_with("… "))
        .unwrap();
    assert_eq!(last.value, "… 50 more");
}

#[test]
fn a_click_chooses_a_frame_and_the_wheel_scrolls_without_choosing() {
    let mut rig = at_a_stop(|adapter, _| {
        let (scopes, _) = adapter.expect_request("scopes");
        adapter.respond(
            scopes,
            "scopes",
            json!({ "scopes": [{ "name": "Locals", "variablesReference": 21 }] }),
        );
        let (variables, _) = adapter.expect_request("variables");
        adapter.respond(variables, "variables", json!({ "variables": [] }));
    });
    rig.draw();
    let row = rig.row_of("caller");
    let column = rig.gutter_of("caller").chars().count() as u16 + 1;

    click(&mut rig, column, row);

    assert_eq!(rig.app.state().focus(), Focus::Debug);
    assert_eq!(rig.app.state().debug().selected_frame, 1);
    rig.wait_for("the variables of the frame", |rig| rows(rig) == ["Locals"]);
    rig.app.handle_input(InputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(
        rig.app.state().debug().selected_frame,
        1,
        "the wheel chooses nothing"
    );
}

#[test]
fn plain_keys_in_the_pane_step_the_program_and_escape_gives_the_keyboard_back() {
    let mut rig = at_a_stop(|adapter, _| {
        let (next, arguments) = adapter.expect_request("next");
        assert_eq!(arguments["threadId"], 1);
        adapter.respond(next, "next", Value::Null);
    });
    chord(&mut rig, 'k', 'v');

    rig.press(KeyCode::Char('n'));
    assert_eq!(
        rig.status(),
        crate::components::debug::model::Status::Running
    );
    rig.press(KeyCode::Esc);
    assert_eq!(rig.app.state().focus(), Focus::Editor);
}

#[test]
fn the_pane_says_what_to_do_when_there_is_no_session() {
    let mut rig = debugging(|adapter, _| while adapter.next().is_some() {});
    chord(&mut rig, 'k', 'v');
    rig.draw();

    assert!(
        rig.screen().contains("no debug session"),
        "{}",
        rig.screen()
    );
}

#[test]
fn a_layout_without_the_pane_says_so() {
    let mut rig = debugging(|adapter, _| while adapter.next().is_some() {});
    rig.app
        .state_mut()
        .set_layout(crate::ui::layout::LayoutTree::from_ron("Pane(view: Editor)").unwrap());

    chord(&mut rig, 'k', 'v');

    assert!(
        rig.said_has("the layout has no Debug view"),
        "{:?}",
        rig.said()
    );
    assert_eq!(rig.app.state().focus(), Focus::Editor);
}

#[test]
fn a_layout_without_the_debug_view_is_said_at_the_first_stop() {
    let mut rig = debugging(|adapter, root| {
        until_the_first_stop(adapter, root);
        while adapter.next().is_some() {}
    });
    rig.app.state_mut().set_layout(
        crate::ui::layout::LayoutTree::from_ron(
            "Col([Pane(view: Editor, sides: [], title: Hidden), Fixed(12, BottomBar(views: [Terminal, Errors])), Fixed(1, Pane(view: StatusBar))])",
        )
        .unwrap(),
    );
    rig.open_file("main.rs");
    rig.breakpoint_on(1);

    rig.app.run_action(Action::Debug);
    rig.wait_for("the notice", |rig| {
        rig.said_has("the layout has no Debug view")
    });
}
