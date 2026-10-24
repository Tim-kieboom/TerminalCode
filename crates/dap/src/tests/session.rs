//! A whole debug session, as an adapter plays it, with the client taking the steps in the
//! order the protocol asks for.

use serde_json::{Value, json};

use crate::test_support::{PATIENCE, fake_adapter, stopped};
use crate::*;

/// Waits for the next event, which must be `want`.
fn expect_event(client: &Client, want: impl Fn(&Event) -> bool, what: &str) -> Event {
    match client.wait_update(PATIENCE) {
        Some(Update::Event(event)) if want(&event) => event,
        other => panic!("expected {what}, got {other:?}"),
    }
}

/// An adapter that answers `launch` only after `configurationDone`, as debugpy does, and the
/// one that answers it at once, as some do: the client must not wait for it before it sends
/// the breakpoints.
fn session_with(answers_launch_first: bool) {
    let (client, fake) = fake_adapter(move |adapter| {
        let arguments = adapter.answer_initialize(json!({
            "supportsConfigurationDoneRequest": true,
            "supportsTerminateRequest": true,
        }));
        assert_eq!(arguments["supportsRunInTerminalRequest"], true);
        let (launch, launching) = adapter.expect_request("launch");
        assert_eq!(launching["program"], "/p/target/debug/app");
        if answers_launch_first {
            adapter.respond(launch, "launch", Value::Null);
        }
        adapter.event("initialized", Value::Null);

        // The breakpoints, then the end of the setup, in this order.
        let (set, arguments) = adapter.expect_request("setBreakpoints");
        assert_eq!(arguments["source"]["path"], "/p/src/main.rs");
        adapter.respond(
            set,
            "setBreakpoints",
            json!({ "breakpoints": [{ "id": 1, "verified": true, "line": 4 }] }),
        );
        let (done, _) = adapter.expect_request("configurationDone");
        adapter.respond(done, "configurationDone", Value::Null);
        if !answers_launch_first {
            adapter.respond(launch, "launch", Value::Null);
        }

        // The program is started in a terminal of the client's.
        let run = adapter.reverse_request(
            "runInTerminal",
            json!({ "cwd": "/p", "args": ["/p/target/debug/app"] }),
        );
        let (answered, outcome) = adapter.expect_response();
        assert_eq!(answered, run);
        assert_eq!(outcome.unwrap()["processId"], 100);

        adapter.event("stopped", stopped("breakpoint", 1));
        let (threads, _) = adapter.expect_request("threads");
        adapter.respond(
            threads,
            "threads",
            json!({ "threads": [{ "id": 1, "name": "main" }] }),
        );
        let (stack, arguments) = adapter.expect_request("stackTrace");
        assert_eq!(arguments["threadId"], 1);
        adapter.respond(stack, "stackTrace", json!({ "stackFrames": [
            { "id": 1000, "name": "app::main", "source": { "path": "/p/src/main.rs" }, "line": 4, "column": 9 },
        ] }));
        let (scopes, arguments) = adapter.expect_request("scopes");
        assert_eq!(arguments["frameId"], 1000);
        adapter.respond(
            scopes,
            "scopes",
            json!({ "scopes": [{ "name": "Locals", "variablesReference": 7 }] }),
        );
        let (variables, arguments) = adapter.expect_request("variables");
        assert_eq!(arguments["variablesReference"], 7);
        adapter.respond(
            variables,
            "variables",
            json!({ "variables": [{ "name": "total", "value": "1", "type": "i32" }] }),
        );

        let (go, _) = adapter.expect_request("continue");
        adapter.respond(go, "continue", json!({ "allThreadsContinued": true }));
        adapter.event("exited", json!({ "exitCode": 0 }));
        adapter.event("terminated", json!({}));
        let (bye, arguments) = adapter.expect_request("disconnect");
        assert_eq!(arguments["terminateDebuggee"], true);
        adapter.respond(bye, "disconnect", Value::Null);
    });

    // Setting up.
    let capabilities = client
        .initialize(&InitializeArguments::new("lldb-dap"))
        .unwrap()
        .wait(PATIENCE)
        .unwrap();
    assert!(capabilities.supports_configuration_done_request);
    let mut launch = client
        .launch(json!({ "program": "/p/target/debug/app", "runInTerminal": true }))
        .unwrap();
    expect_event(&client, |event| *event == Event::Initialized, "initialized");
    let set = client
        .set_breakpoints(&SetBreakpointsArguments {
            source: Source::file("/p/src/main.rs"),
            breakpoints: vec![SourceBreakpoint::at(4)],
        })
        .unwrap()
        .wait(PATIENCE)
        .unwrap();
    assert!(set.breakpoints[0].verified);
    client.configuration_done().unwrap().wait(PATIENCE).unwrap();
    // Only now is the answer to launch waited for.
    loop {
        if let Some(answer) = launch.try_take() {
            answer.unwrap();
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }

    // The adapter asks for the program to be started.
    let Some(Update::Reverse {
        seq,
        request: ReverseRequest::RunInTerminal(run),
    }) = client.wait_update(PATIENCE)
    else {
        panic!("no runInTerminal");
    };
    assert_eq!(run.args, ["/p/target/debug/app"]);
    client
        .respond_run_in_terminal(
            seq,
            Ok(RunInTerminalResponse {
                process_id: Some(100),
                shell_process_id: None,
            }),
        )
        .unwrap();

    // Stopped on the breakpoint: look around.
    let Event::Stopped(stop) = expect_event(&client, |e| matches!(e, Event::Stopped(_)), "stopped")
    else {
        unreachable!();
    };
    assert_eq!(
        (stop.reason.as_str(), stop.thread_id),
        ("breakpoint", Some(1))
    );
    let threads = client.threads().unwrap().wait(PATIENCE).unwrap();
    assert_eq!(threads.threads[0].name, "main");
    let stack = client
        .stack_trace(&StackTraceArguments {
            thread_id: 1,
            ..StackTraceArguments::default()
        })
        .unwrap()
        .wait(PATIENCE)
        .unwrap();
    assert_eq!(stack.stack_frames[0].line, 4);
    let scopes = client
        .scopes(stack.stack_frames[0].id)
        .unwrap()
        .wait(PATIENCE)
        .unwrap();
    let variables = client
        .variables(&VariablesArguments::of(
            scopes.scopes[0].variables_reference,
        ))
        .unwrap()
        .wait(PATIENCE)
        .unwrap();
    assert_eq!(variables.variables[0].name, "total");

    // Go on to the end.
    assert!(
        client
            .continue_(1)
            .unwrap()
            .wait(PATIENCE)
            .unwrap()
            .all_threads_continued
    );
    expect_event(
        &client,
        |event| matches!(event, Event::Exited { exit_code: 0 }),
        "exited",
    );
    expect_event(&client, |event| *event == Event::Terminated, "terminated");
    client.disconnect(true).unwrap().wait(PATIENCE).unwrap();
    fake.finish();
}

#[test]
fn a_session_in_which_launch_is_answered_after_the_setup() {
    session_with(false);
}

#[test]
fn a_session_in_which_launch_is_answered_at_once() {
    session_with(true);
}

#[test]
fn a_program_that_ends_by_itself_ends_the_session_with_its_events() {
    let (client, fake) = fake_adapter(|adapter| {
        adapter.event("exited", json!({ "exitCode": 7 }));
        adapter.event("terminated", Value::Null);
    });

    expect_event(
        &client,
        |event| *event == Event::Exited { exit_code: 7 },
        "exited",
    );
    expect_event(&client, |event| *event == Event::Terminated, "terminated");
    fake.finish();
}
