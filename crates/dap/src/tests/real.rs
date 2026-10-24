//! A real debug adapter: `lldb-dap` (LLVM). It is not installed everywhere, so these are
//! ignored unless asked for:
//!
//! ```text
//! cargo test -p trcode-dap real -- --ignored
//! TRCODE_LLDB_DAP=/path/to/lldb-dap cargo test -p trcode-dap real -- --ignored
//! ```
//!
//! They need `rustc` too (a small program is built with debug information). Both start the
//! program: by the adapter, and in a terminal of the client's (`runInTerminal`, which is how
//! the editor does it).

use std::path::Path;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use serde_json::json;

use crate::*;

const PROGRAM: &str = "fn main() {\n    let mut total = 0;\n    for i in 1..=3 {\n        total += i;\n    }\n    println!(\"total = {total}\");\n}\n";
/// The line of `total += i`.
const BREAKPOINT_LINE: i64 = 4;
/// Time for the debugger to start, read the program and stop at the breakpoint.
const SLOW: Duration = Duration::from_secs(60);
/// How long one look for the next thing the adapter does lasts.
const LOOK: Duration = Duration::from_millis(50);

fn adapter() -> String {
    std::env::var("TRCODE_LLDB_DAP").unwrap_or_else(|_| "lldb-dap".to_owned())
}

/// Builds the program with debug information; its path and the file it was made from.
fn build(dir: &Path) -> (String, String) {
    let source = dir.join("main.rs");
    std::fs::write(&source, PROGRAM).unwrap();
    let program = dir.join("program");
    let status = Command::new("rustc")
        .args(["-g", "-C", "opt-level=0", "-o"])
        .arg(&program)
        .arg(&source)
        .status()
        .expect("rustc");
    assert!(status.success(), "rustc failed");
    (
        program.to_string_lossy().into_owned(),
        source.to_string_lossy().into_owned(),
    )
}

/// A session as the editor takes it: it looks at what the adapter does, and starts the program
/// when the adapter asks.
struct Session<'a> {
    client: &'a Client,
    started: Vec<Child>,
    /// Events that came while something else was waited for. An adapter may send an event (the
    /// program stopped) before the answer to the request that made it happen (`continue`), so
    /// an event must be kept until it is asked for.
    backlog: std::collections::VecDeque<Event>,
}

impl Session<'_> {
    /// One look at what the adapter does: an event is put in the backlog, and a request of the
    /// adapter is done on the way.
    fn look(&mut self) {
        let Some(update) = self.client.wait_update(LOOK) else {
            return;
        };
        match update {
            Update::Event(event) => self.backlog.push_back(event),
            Update::Reverse {
                seq,
                request: ReverseRequest::RunInTerminal(run),
            } => {
                let mut command = Command::new(&run.args[0]);
                command.args(&run.args[1..]);
                if !run.cwd.is_empty() {
                    command.current_dir(&run.cwd);
                }
                let child = command.spawn().expect("the program the adapter asked for");
                let response = RunInTerminalResponse {
                    process_id: Some(i64::from(child.id())),
                    shell_process_id: None,
                };
                self.started.push(child);
                self.client
                    .respond_run_in_terminal(seq, Ok(response))
                    .unwrap();
            }
            Update::Closed(reason) => panic!("the adapter went away: {reason}"),
        }
    }

    /// Waits for the event `want` takes: the first in the backlog, or the first to come. The
    /// events before it are let go.
    fn event(&mut self, want: impl Fn(&Event) -> bool, what: &str) -> Event {
        let until = Instant::now() + SLOW;
        while Instant::now() < until {
            while let Some(event) = self.backlog.pop_front() {
                if want(&event) {
                    return event;
                }
            }
            self.look();
        }
        panic!("timed out waiting for {what}");
    }

    /// Waits for the answer to a request while the adapter is looked after.
    fn answer<T: serde::de::DeserializeOwned>(
        &mut self,
        mut reply: PendingReply<T>,
        what: &str,
    ) -> T {
        let until = Instant::now() + SLOW;
        while Instant::now() < until {
            if let Some(answer) = reply.try_take() {
                return answer.unwrap_or_else(|error| panic!("{what}: {error}"));
            }
            self.look();
        }
        panic!("timed out waiting for the answer to {what}");
    }
}

fn debug(in_terminal: bool) {
    let dir = tempfile::tempdir().unwrap();
    let (program, source) = build(dir.path());
    let client = Client::spawn(&adapter(), &[], || {}).expect("lldb-dap is installed");
    let mut session = Session {
        client: &client,
        started: Vec::new(),
        backlog: std::collections::VecDeque::new(),
    };

    let capabilities = client
        .initialize(&InitializeArguments::new("lldb-dap"))
        .unwrap()
        .wait(SLOW)
        .unwrap();
    assert!(capabilities.supports_configuration_done_request);
    let launch = client
        .launch(json!({
            "program": program, "args": [], "cwd": dir.path(),
            "stopOnEntry": false, "runInTerminal": in_terminal,
        }))
        .unwrap();
    session.event(|event| *event == Event::Initialized, "initialized");
    let set = client
        .set_breakpoints(&SetBreakpointsArguments {
            source: Source::file(&source),
            breakpoints: vec![SourceBreakpoint::at(BREAKPOINT_LINE)],
        })
        .unwrap()
        .wait(SLOW)
        .unwrap();
    assert!(set.breakpoints[0].verified, "{set:?}");
    let done = client.configuration_done().unwrap();
    session.answer(done, "configurationDone");
    session.answer(launch, "launch");

    // Stopped on the breakpoint, in the loop.
    let Event::Stopped(stop) = session.event(|e| matches!(e, Event::Stopped(_)), "a stop") else {
        unreachable!();
    };
    assert_eq!(stop.reason, "breakpoint");
    let thread_id = stop.thread_id.expect("the thread that stopped");
    let stack = session.answer(
        client
            .stack_trace(&StackTraceArguments {
                thread_id,
                ..StackTraceArguments::default()
            })
            .unwrap(),
        "stackTrace",
    );
    let top = &stack.stack_frames[0];
    assert_eq!(top.line, BREAKPOINT_LINE, "{top:?}");
    assert!(top.name.contains("main"), "{top:?}");
    let scopes = session.answer(client.scopes(top.id).unwrap(), "scopes");
    let locals = &scopes.scopes[0];
    let variables = session.answer(
        client
            .variables(&VariablesArguments::of(locals.variables_reference))
            .unwrap(),
        "variables",
    );
    assert!(
        variables
            .variables
            .iter()
            .any(|variable| variable.name == "total"),
        "{variables:?}"
    );

    // On to the end of the program: the breakpoint is in a loop of three, so it stops twice
    // more.
    let mut stops = 1;
    loop {
        session.answer(client.continue_(thread_id).unwrap(), "continue");
        let event = session.event(
            |event| matches!(event, Event::Stopped(_) | Event::Exited { .. }),
            "the program to stop or end",
        );
        match event {
            Event::Stopped(_) => stops += 1,
            Event::Exited { exit_code } => {
                assert_eq!(exit_code, 0);
                break;
            }
            _ => unreachable!(),
        }
    }
    assert_eq!(
        stops, 3,
        "the breakpoint is hit once for each turn of the loop"
    );
    session.event(|event| *event == Event::Terminated, "the session to end");
    session.answer(client.disconnect(true).unwrap(), "disconnect");
    assert_eq!(
        session.started.len(),
        usize::from(in_terminal),
        "the program is started by the client exactly when the adapter was asked to"
    );
    for mut child in session.started {
        let _ = child.wait();
    }
}

#[test]
#[ignore = "needs lldb-dap and rustc"]
fn lldb_dap_stops_at_a_breakpoint_and_shows_the_stack_and_the_variables() {
    debug(false);
}

#[test]
#[ignore = "needs lldb-dap and rustc"]
fn lldb_dap_asks_for_the_program_to_be_started_in_a_terminal_and_stops_the_same() {
    debug(true);
}
