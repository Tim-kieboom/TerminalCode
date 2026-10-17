use serde_json::{Value, json};

use crate::code_action::{execute_offered, offered, request, request_execute, request_resolve};
use crate::diagnostics::Severity;
use crate::position::LspPosition;
use crate::test_support::{PATIENCE, connect};
use crate::{
    CodeAction, Command, ContextDiagnostic, LspError, parse_code_action, parse_code_actions,
};

fn at(line: u32, character: u32) -> LspPosition {
    LspPosition { line, character }
}

fn insert_edit() -> Value {
    json!({ "changes": { "file:///p/a.rs": [{
        "range": {
            "start": { "line": 0, "character": 0 },
            "end": { "line": 0, "character": 0 },
        },
        "newText": "use std::collections::HashMap;\n",
    }] } })
}

#[test]
fn nothing_is_no_actions() {
    assert!(parse_code_actions(&Value::Null).unwrap().is_empty());
    assert!(parse_code_actions(&json!([])).unwrap().is_empty());
}

#[test]
fn an_action_with_an_edit_is_read() {
    let answer = json!([{
        "title": "Import `HashMap`",
        "kind": "quickfix",
        "isPreferred": true,
        "edit": insert_edit(),
    }]);

    let found = parse_code_actions(&answer).unwrap();

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].title, "Import `HashMap`");
    assert_eq!(found[0].kind.as_deref(), Some("quickfix"));
    assert!(found[0].preferred);
    let edit = found[0].edit.as_ref().unwrap();
    assert_eq!(edit.files[0].uri, "file:///p/a.rs");
    assert_eq!(
        edit.files[0].edits[0].new_text,
        "use std::collections::HashMap;\n"
    );
    assert!(found[0].command.is_none() && found[0].disabled.is_none());
}

#[test]
fn an_action_with_only_a_command_is_read() {
    let answer = json!([{
        "title": "Run it",
        "command": { "title": "Run", "command": "rust-analyzer.run", "arguments": [1, "a"] },
    }]);

    let found = parse_code_actions(&answer).unwrap();

    assert!(found[0].edit.is_none());
    let command = found[0].command.as_ref().unwrap();
    assert_eq!(command.command, "rust-analyzer.run");
    assert_eq!(command.arguments, [json!(1), json!("a")]);
}

#[test]
fn a_bare_command_is_an_action_that_runs_it() {
    let answer = json!([{ "title": "Fix all", "command": "fix.all", "arguments": [true] }]);

    let found = parse_code_actions(&answer).unwrap();

    assert_eq!(found[0].title, "Fix all");
    let command = found[0].command.as_ref().unwrap();
    assert_eq!(
        (command.command.as_str(), command.arguments.len()),
        ("fix.all", 1)
    );
}

#[test]
fn an_action_that_waits_for_resolve_has_no_edit_yet() {
    let answer = json!([{ "title": "Add arms", "kind": "quickfix", "data": { "id": 7 } }]);

    let found = parse_code_actions(&answer).unwrap();

    assert!(found[0].edit.is_none() && found[0].command.is_none());
    assert_eq!(found[0].raw["data"], json!({ "id": 7 }));
}

#[test]
fn a_disabled_action_says_why() {
    let answer = json!([{ "title": "Inline", "disabled": { "reason": "no uses" } }]);

    let found = parse_code_actions(&answer).unwrap();

    assert_eq!(found[0].disabled.as_deref(), Some("no uses"));
}

#[test]
fn an_item_without_a_title_or_with_a_broken_edit_is_left_out() {
    let answer = json!([
        { "kind": "quickfix" },
        { "title": "Broken", "edit": { "changes": { "file:///p/a.rs": 3 } } },
        { "title": "Fine" },
        7,
    ]);

    let found = parse_code_actions(&answer).unwrap();

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].title, "Fine");
}

#[test]
fn an_answer_of_another_shape_is_an_error_naming_the_method() {
    let Err(LspError::Unexpected { method, .. }) = parse_code_actions(&json!("a")) else {
        panic!("a string was read as a list");
    };
    assert_eq!(method, "textDocument/codeAction");
}

#[test]
fn a_server_says_what_it_offers() {
    assert!(offered(&json!({ "codeActionProvider": true })));
    assert!(offered(
        &json!({ "codeActionProvider": { "resolveProvider": true } })
    ));
    assert!(!offered(&json!({ "codeActionProvider": false })));
    assert!(!offered(&json!({})));
    assert!(super::super::code_action::resolve_offered(
        &json!({ "codeActionProvider": { "resolveProvider": true } })
    ));
    assert!(!super::super::code_action::resolve_offered(
        &json!({ "codeActionProvider": true })
    ));
    assert!(execute_offered(
        &json!({ "executeCommandProvider": { "commands": [] } })
    ));
    assert!(!execute_offered(&json!({})));
}

fn wait<T>(mut take: impl FnMut() -> Option<T>) -> T {
    let deadline = std::time::Instant::now() + PATIENCE;
    loop {
        if let Some(answer) = take() {
            return answer;
        }
        assert!(std::time::Instant::now() < deadline, "no answer");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn the_requests_carry_the_range_the_problems_and_the_action() {
    let rig = connect(|server| {
        server.start_up();
        let (id, params) = server.expect_request("textDocument/codeAction");
        assert_eq!(params["textDocument"], json!({ "uri": "file:///p/a.rs" }));
        assert_eq!(
            params["range"]["start"],
            json!({ "line": 2, "character": 4 })
        );
        assert_eq!(params["range"]["end"], json!({ "line": 2, "character": 9 }));
        let problem = &params["context"]["diagnostics"][0];
        assert_eq!(problem["message"], json!("cannot find `HashMap`"));
        assert_eq!(problem["severity"], json!(1));
        assert_eq!(problem["code"], json!("E0433"));
        assert_eq!(problem["source"], json!("rustc"));
        assert_eq!(params["context"]["triggerKind"], json!(1));
        server.respond(id, json!([{ "title": "Import", "data": 5 }]));

        let (id, params) = server.expect_request("codeAction/resolve");
        assert_eq!(params["data"], json!(5));
        server.respond(
            id,
            json!({ "title": "Import", "data": 5, "edit": insert_edit() }),
        );

        let (id, params) = server.expect_request("workspace/executeCommand");
        assert_eq!(params["command"], json!("fix.all"));
        assert_eq!(params["arguments"], json!([1]));
        server.respond(id, Value::Null);
    });
    crate::initialize(
        &rig.connection,
        std::path::Path::new(super::handshake_tests::root_for_tests()),
        &Value::Null,
        PATIENCE,
    )
    .unwrap();

    let problem = ContextDiagnostic {
        start: at(2, 4),
        end: at(2, 9),
        severity: Severity::Error,
        message: "cannot find `HashMap`".to_owned(),
        source: Some("rustc".to_owned()),
        code: Some("E0433".to_owned()),
    };
    let mut asked = request(
        &rig.connection,
        "file:///p/a.rs",
        at(2, 4),
        at(2, 9),
        &[problem],
    )
    .unwrap();
    let actions = wait(|| asked.try_take()).unwrap();
    assert_eq!(actions[0].title, "Import");

    let mut resolving = request_resolve(&rig.connection, &actions[0]).unwrap();
    let resolved: CodeAction = wait(|| resolving.try_take()).unwrap();
    assert!(resolved.edit.is_some());

    let command = Command {
        title: "Fix".to_owned(),
        command: "fix.all".to_owned(),
        arguments: vec![json!(1)],
    };
    let mut running = request_execute(&rig.connection, &command).unwrap();
    wait(|| running.try_take()).unwrap();
    rig.server.finish();
}

#[test]
fn the_resolved_answer_is_read_as_an_action() {
    let found = parse_code_action(&json!({ "title": "T", "edit": insert_edit() }));

    assert!(found.unwrap().edit.is_some());
}

#[test]
fn the_client_declares_code_actions_with_resolve_and_commands() {
    let capabilities = crate::handshake::client_capabilities();

    let action = &capabilities["textDocument"]["codeAction"];
    assert_eq!(action["resolveSupport"]["properties"], json!(["edit"]));
    assert_eq!(action["isPreferredSupport"], json!(true));
    assert!(
        action["codeActionLiteralSupport"]["codeActionKind"]["valueSet"]
            .as_array()
            .unwrap()
            .contains(&json!("quickfix"))
    );
    assert!(capabilities["workspace"]["executeCommand"].is_object());
}
