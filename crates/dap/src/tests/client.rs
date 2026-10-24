//! What each request sends and how its answer is read, and the events and requests the adapter
//! makes, through the scripted adapter.

use serde_json::{Value, json};

use crate::test_support::{PATIENCE, fake_adapter};
use crate::*;

/// Runs a client against an adapter that expects `command`, checks the arguments with
/// `check`, and answers with `body`.
fn answered<T>(
    command: &'static str,
    body: Value,
    check: impl FnOnce(&Value) + Send + 'static,
    ask: impl FnOnce(&Client) -> Result<PendingReply<T>, DapError>,
) -> T
where
    T: serde::de::DeserializeOwned,
{
    let (client, fake) = fake_adapter(move |adapter| {
        let (seq, arguments) = adapter.expect_request(command);
        check(&arguments);
        adapter.respond(seq, command, body);
    });
    let answer = ask(&client).unwrap().wait(PATIENCE).unwrap();
    fake.finish();
    answer
}

#[test]
fn initialize_says_who_the_client_is_and_reads_what_the_adapter_can_do() {
    let capabilities = answered(
        "initialize",
        json!({ "supportsConfigurationDoneRequest": true, "supportsTerminateRequest": true,
                "supportsSomethingNew": true }),
        |arguments| {
            assert_eq!(arguments["adapterID"], "lldb-dap");
            assert_eq!(arguments["clientID"], "trcode");
            assert_eq!(arguments["linesStartAt1"], true);
            assert_eq!(arguments["columnsStartAt1"], true);
            assert_eq!(arguments["pathFormat"], "path");
            assert_eq!(arguments["supportsRunInTerminalRequest"], true);
        },
        |client| client.initialize(&InitializeArguments::new("lldb-dap")),
    );

    assert!(capabilities.supports_configuration_done_request);
    assert!(capabilities.supports_terminate_request);
    assert!(
        !capabilities.supports_restart_request,
        "what it did not say is off"
    );
}

#[test]
fn launch_sends_the_adapters_own_arguments_untouched() {
    answered::<Nothing>(
        "launch",
        json!({}),
        |arguments| {
            assert_eq!(
                arguments,
                &json!({ "program": "/p/app", "args": ["-v"], "x": null })
            )
        },
        |client| client.launch(json!({ "program": "/p/app", "args": ["-v"], "x": null })),
    );
}

#[test]
fn configuration_done_terminate_and_disconnect() {
    answered::<Nothing>(
        "configurationDone",
        Value::Null,
        |arguments| {
            assert!(arguments.is_null());
        },
        |client| client.configuration_done(),
    );
    answered::<Nothing>(
        "terminate",
        Value::Null,
        |_| {},
        |client| client.terminate(),
    );
    answered::<Nothing>(
        "disconnect",
        Value::Null,
        |arguments| assert_eq!(arguments, &json!({ "terminateDebuggee": true })),
        |client| client.disconnect(true),
    );
}

#[test]
fn set_breakpoints_sends_a_file_with_all_its_breakpoints_and_reads_what_the_adapter_made_of_them() {
    let answer = answered(
        "setBreakpoints",
        json!({ "breakpoints": [
            { "id": 1, "verified": true, "line": 12 },
            { "verified": false, "message": "no code at that line" },
        ] }),
        |arguments| {
            assert_eq!(
                arguments,
                &json!({
                    "source": { "path": "/p/src/main.rs" },
                    "breakpoints": [
                        { "line": 12 },
                        { "line": 20, "condition": "i == 3" },
                    ],
                })
            );
        },
        |client| {
            client.set_breakpoints(&SetBreakpointsArguments {
                source: Source::file("/p/src/main.rs"),
                breakpoints: vec![
                    SourceBreakpoint::at(12),
                    SourceBreakpoint {
                        condition: Some("i == 3".into()),
                        ..SourceBreakpoint::at(20)
                    },
                ],
            })
        },
    );

    assert_eq!(answer.breakpoints.len(), 2);
    assert!(answer.breakpoints[0].verified);
    assert_eq!(answer.breakpoints[0].line, Some(12));
    assert!(!answer.breakpoints[1].verified);
    assert_eq!(
        answer.breakpoints[1].message.as_deref(),
        Some("no code at that line")
    );
}

#[test]
fn the_steps_and_continue_name_the_thread() {
    for (command, call) in [
        ("next", (|c: &Client| c.next(4)) as fn(&Client) -> _),
        ("stepIn", |c| c.step_in(4)),
        ("stepOut", |c| c.step_out(4)),
        ("pause", |c| c.pause(4)),
    ] {
        answered::<Nothing>(
            command,
            Value::Null,
            |arguments| assert_eq!(arguments, &json!({ "threadId": 4 })),
            call,
        );
    }
    let continued = answered(
        "continue",
        json!({}),
        |arguments| assert_eq!(arguments, &json!({ "threadId": 4 })),
        |client| client.continue_(4),
    );
    assert!(
        continued.all_threads_continued,
        "an adapter that says nothing means all of them"
    );
}

#[test]
fn threads_stack_scopes_and_variables_are_read() {
    let threads = answered(
        "threads",
        json!({ "threads": [{ "id": 1, "name": "main" }, { "id": 2 }] }),
        |_| {},
        |client| client.threads(),
    );
    assert_eq!(threads.threads.len(), 2);
    assert_eq!(threads.threads[1].name, "");

    let stack = answered(
        "stackTrace",
        json!({ "stackFrames": [
            { "id": 1000, "name": "main", "source": { "path": "/p/src/main.rs", "name": "main.rs" },
              "line": 5, "column": 3 },
            { "id": 1001, "name": "start" },
        ], "totalFrames": 2 }),
        |arguments| assert_eq!(arguments, &json!({ "threadId": 1, "levels": 20 })),
        |client| {
            client.stack_trace(&StackTraceArguments {
                thread_id: 1,
                levels: Some(20),
                ..StackTraceArguments::default()
            })
        },
    );
    assert_eq!(stack.total_frames, Some(2));
    assert_eq!(stack.stack_frames[0].line, 5);
    assert_eq!(
        stack.stack_frames[0]
            .source
            .as_ref()
            .and_then(|s| s.path.as_deref()),
        Some("/p/src/main.rs")
    );
    assert!(
        stack.stack_frames[1].source.is_none(),
        "a frame with no file"
    );

    let scopes = answered(
        "scopes",
        json!({ "scopes": [{ "name": "Locals", "variablesReference": 7 },
                           { "name": "Registers", "variablesReference": 8, "expensive": true }] }),
        |arguments| assert_eq!(arguments, &json!({ "frameId": 1000 })),
        |client| client.scopes(1000),
    );
    assert_eq!(scopes.scopes[0].variables_reference, 7);
    assert!(scopes.scopes[1].expensive && !scopes.scopes[0].expensive);

    let variables = answered(
        "variables",
        json!({ "variables": [
            { "name": "x", "value": "5", "type": "i32" },
            { "name": "v", "value": "Vec(3)", "variablesReference": 9, "indexedVariables": 3 },
        ] }),
        |arguments| {
            assert_eq!(
                arguments,
                &json!({ "variablesReference": 7, "start": 10, "count": 5 })
            );
        },
        |client| client.variables(&VariablesArguments::page(7, 10, 5)),
    );
    assert_eq!(variables.variables[0].type_name.as_deref(), Some("i32"));
    assert_eq!(variables.variables[0].variables_reference, 0, "no children");
    assert_eq!(variables.variables[1].variables_reference, 9);
    assert_eq!(variables.variables[1].indexed_variables, Some(3));
}

#[test]
fn all_the_children_are_asked_for_without_a_range() {
    answered::<VariablesResponse>(
        "variables",
        json!({}),
        |arguments| assert_eq!(arguments, &json!({ "variablesReference": 7 })),
        |client| client.variables(&VariablesArguments::of(7)),
    );
}

#[test]
fn the_events_are_read_into_what_they_mean() {
    let cases: Vec<(&str, Value, Event)> = vec![
        ("initialized", Value::Null, Event::Initialized),
        (
            "stopped",
            json!({ "reason": "breakpoint", "threadId": 1, "allThreadsStopped": true,
                    "hitBreakpointIds": [3], "description": "Breakpoint 3" }),
            Event::Stopped(Stopped {
                reason: "breakpoint".into(),
                description: Some("Breakpoint 3".into()),
                text: None,
                thread_id: Some(1),
                all_threads_stopped: true,
                hit_breakpoint_ids: vec![3],
            }),
        ),
        (
            "continued",
            json!({ "threadId": 1 }),
            Event::Continued(Continued {
                thread_id: 1,
                all_threads_continued: true,
            }),
        ),
        (
            "exited",
            json!({ "exitCode": 3 }),
            Event::Exited { exit_code: 3 },
        ),
        ("terminated", json!({}), Event::Terminated),
        (
            "thread",
            json!({ "reason": "started", "threadId": 2 }),
            Event::Thread(ThreadEvent {
                reason: "started".into(),
                thread_id: 2,
            }),
        ),
        (
            "output",
            json!({ "output": "hello\n" }),
            Event::Output(Output {
                category: None,
                output: "hello\n".into(),
            }),
        ),
    ];
    for (name, body, expected) in cases {
        assert_eq!(Event::parse(name, body), expected, "{name}");
    }
    let Event::Breakpoint(changed) = Event::parse(
        "breakpoint",
        json!({ "reason": "changed", "breakpoint": { "id": 1, "verified": true, "line": 9 } }),
    ) else {
        panic!("not a breakpoint event");
    };
    assert_eq!(changed.reason, "changed");
    assert!(changed.breakpoint.verified);
}

#[test]
fn an_event_that_is_not_known_or_not_as_it_should_be_is_passed_on_as_it_is() {
    for (name, body) in [
        ("module", json!({ "reason": "new" })),
        ("stopped", json!("not an object")),
        ("exited", json!({})),
        ("thread", json!({ "reason": "started" })),
    ] {
        assert_eq!(
            Event::parse(name, body.clone()),
            Event::Other {
                event: name.into(),
                body
            },
            "{name}"
        );
    }
}

#[test]
fn a_run_in_terminal_request_is_handed_over_and_answered_with_the_process() {
    let (client, fake) = fake_adapter(|adapter| {
        let seq = adapter.reverse_request(
            "runInTerminal",
            json!({ "kind": "integrated", "title": "app", "cwd": "/p",
                    "args": ["/p/target/debug/app", "--fast"],
                    "env": { "RUST_LOG": "debug", "REMOVED": null } }),
        );
        let (request_seq, outcome) = adapter.expect_response();
        assert_eq!(request_seq, seq);
        assert_eq!(outcome.unwrap(), json!({ "processId": 4242 }));
    });

    let Some(Update::Reverse {
        seq,
        request: ReverseRequest::RunInTerminal(run),
    }) = client.wait_update(PATIENCE)
    else {
        panic!("no runInTerminal");
    };
    assert_eq!(run.cwd, "/p");
    assert_eq!(run.args, ["/p/target/debug/app", "--fast"]);
    assert_eq!(run.env["RUST_LOG"].as_deref(), Some("debug"));
    assert_eq!(run.env["REMOVED"], None, "a variable that is to be removed");
    assert_eq!(run.title.as_deref(), Some("app"));
    client
        .respond_run_in_terminal(
            seq,
            Ok(RunInTerminalResponse {
                process_id: Some(4242),
                shell_process_id: None,
            }),
        )
        .unwrap();
    fake.finish();
}

#[test]
fn a_run_in_terminal_that_could_not_be_done_is_answered_with_why() {
    let (client, fake) = fake_adapter(|adapter| {
        adapter.reverse_request("runInTerminal", json!({ "cwd": "/p", "args": ["x"] }));
        let (_, outcome) = adapter.expect_response();
        assert_eq!(outcome.unwrap_err().message, "no terminal is free");
    });

    let Some(Update::Reverse { seq, .. }) = client.wait_update(PATIENCE) else {
        panic!("no request");
    };
    client
        .respond_run_in_terminal(seq, Err("no terminal is free".into()))
        .unwrap();
    fake.finish();
}

#[test]
fn a_request_of_the_adapter_that_is_not_known_is_refused_and_never_left_waiting() {
    let (client, fake) = fake_adapter(|adapter| {
        let strange = adapter.reverse_request("startDebugging", json!({ "request": "launch" }));
        let (answered, outcome) = adapter.expect_response();
        assert_eq!(answered, strange);
        assert_eq!(outcome.unwrap_err().message, "unsupported");
        // A run in terminal with nonsense arguments is refused too.
        let broken = adapter.reverse_request("runInTerminal", json!({ "args": 5 }));
        let (answered, outcome) = adapter.expect_response();
        assert_eq!(answered, broken);
        assert!(outcome.is_err());
        adapter.event("initialized", Value::Null);
    });

    // Neither request is an update; the event after them is.
    let update = client.wait_update(PATIENCE);

    assert!(
        matches!(update, Some(Update::Event(Event::Initialized))),
        "{update:?}"
    );
    fake.finish();
}

#[test]
fn evaluate_asks_for_an_expression_in_a_frame_for_a_context_and_reads_the_children() {
    let answer = answered(
        "evaluate",
        json!({ "result": "Vec<i32> @ 0x10", "type": "Vec<i32>", "variablesReference": 5,
                "indexedVariables": 3 }),
        |arguments| {
            assert_eq!(arguments["expression"], "v");
            assert_eq!(arguments["frameId"], 1000);
            assert_eq!(arguments["context"], "hover");
        },
        |client| client.evaluate(&EvaluateArguments::hover("v", 1000)),
    );

    assert_eq!(answer.result, "Vec<i32> @ 0x10");
    assert_eq!(answer.type_name.as_deref(), Some("Vec<i32>"));
    assert_eq!(answer.variables_reference, 5);
    assert_eq!(answer.indexed_variables, Some(3));
}

#[test]
fn a_refused_evaluation_says_what_the_adapter_said() {
    let (client, fake) = fake_adapter(|adapter| {
        let (seq, _) = adapter.expect_request("evaluate");
        adapter.refuse(
            seq,
            "evaluate",
            "<user expression 0>:1:3: unrecognized token",
            Value::Null,
        );
    });

    let error = client
        .evaluate(&EvaluateArguments::hover("x + y", 1))
        .unwrap()
        .wait(PATIENCE)
        .unwrap_err();
    fake.finish();

    let DapError::Rejected { rejection, .. } = error else {
        panic!("{error}");
    };
    assert_eq!(
        rejection.describe(),
        "<user expression 0>:1:3: unrecognized token"
    );
}
