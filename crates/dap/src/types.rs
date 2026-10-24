//! The parts of the Debug Adapter Protocol the editor uses: what the adapter can do, the
//! events it sends, the requests it makes of the client, and the arguments and answers of the
//! requests the client makes. Names are the protocol's, in camel case on the wire; fields the
//! editor does not use are left out, and fields an adapter may leave out have defaults.
//! Lines and columns count from 1, as [`InitializeArguments::new`] asks for.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// An answer whose body is of no interest: whatever it is (nothing, `{}`), it is read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Nothing;

impl<'de> Deserialize<'de> for Nothing {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        serde::de::IgnoredAny::deserialize(deserializer)?;
        Ok(Self)
    }
}

// --- starting ---

/// What the client tells the adapter about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeArguments {
    #[serde(rename = "clientID")]
    pub client_id: String,
    pub client_name: String,
    #[serde(rename = "adapterID")]
    pub adapter_id: String,
    pub lines_start_at1: bool,
    pub columns_start_at1: bool,
    pub path_format: String,
    pub supports_variable_type: bool,
    /// The editor starts the program of a debug session in a terminal tab when the adapter asks
    /// ([`ReverseRequest::RunInTerminal`]).
    pub supports_run_in_terminal_request: bool,
}

impl InitializeArguments {
    /// The arguments for the adapter called `adapter_id` (`lldb-dap`).
    pub fn new(adapter_id: &str) -> Self {
        Self {
            client_id: "trcode".to_owned(),
            client_name: "TerminalCode".to_owned(),
            adapter_id: adapter_id.to_owned(),
            lines_start_at1: true,
            columns_start_at1: true,
            path_format: "path".to_owned(),
            supports_variable_type: true,
            supports_run_in_terminal_request: true,
        }
    }
}

/// What the adapter can do (the part the editor looks at).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Capabilities {
    /// The session is set up with `configurationDone` after the breakpoints are sent.
    pub supports_configuration_done_request: bool,
    pub supports_terminate_request: bool,
    pub supports_restart_request: bool,
    pub supports_conditional_breakpoints: bool,
    pub supports_hit_conditional_breakpoints: bool,
    pub supports_log_points: bool,
    pub supports_set_variable: bool,
}

// --- what an adapter sends ---

/// An event, as the editor reads it. One that is not known, or is not as the protocol says, is
/// [`Event::Other`]: it is passed on and never an error.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The adapter is ready for the breakpoints (and `configurationDone`).
    Initialized,
    Stopped(Stopped),
    Continued(Continued),
    /// The program ended.
    Exited {
        exit_code: i64,
    },
    /// The debug session is over.
    Terminated,
    Thread(ThreadEvent),
    Output(Output),
    Breakpoint(BreakpointEvent),
    Other {
        event: String,
        body: Value,
    },
}

impl Event {
    /// Reads the event `name` with its `body`.
    pub fn parse(name: &str, body: Value) -> Self {
        let known = match name {
            "initialized" => Some(Self::Initialized),
            "stopped" => from(&body).map(Self::Stopped),
            "continued" => from(&body).map(Self::Continued),
            "exited" => body["exitCode"]
                .as_i64()
                .map(|exit_code| Self::Exited { exit_code }),
            "terminated" => Some(Self::Terminated),
            "thread" => from(&body).map(Self::Thread),
            "output" => from(&body).map(Self::Output),
            "breakpoint" => from(&body).map(Self::Breakpoint),
            _ => None,
        };
        known.unwrap_or_else(|| Self::Other {
            event: name.to_owned(),
            body,
        })
    }
}

fn from<T: serde::de::DeserializeOwned>(body: &Value) -> Option<T> {
    serde_json::from_value(body.clone()).ok()
}

/// The program stopped.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Stopped {
    /// `step`, `breakpoint`, `exception`, `pause`, `entry`, ... (an adapter may say others).
    pub reason: String,
    pub description: Option<String>,
    pub text: Option<String>,
    pub thread_id: Option<i64>,
    pub all_threads_stopped: bool,
    pub hit_breakpoint_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Continued {
    pub thread_id: i64,
    #[serde(default = "yes")]
    pub all_threads_continued: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadEvent {
    /// `started` or `exited`.
    pub reason: String,
    pub thread_id: i64,
}

/// Something the adapter or the program printed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Output {
    /// `console`, `stdout`, `stderr`, `telemetry`; none is the console.
    #[serde(default)]
    pub category: Option<String>,
    pub output: String,
}

/// A breakpoint changed (it was verified, moved or removed).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BreakpointEvent {
    /// `changed`, `new` or `removed`.
    pub reason: String,
    pub breakpoint: Breakpoint,
}

// --- what an adapter asks of the client ---

/// A request the adapter makes of the client.
#[derive(Debug, Clone, PartialEq)]
pub enum ReverseRequest {
    /// Start a program in a terminal of the client's.
    RunInTerminal(RunInTerminalArguments),
}

/// What to start: `args[0]` is the program. A variable of `env` that is `None` is unset.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RunInTerminalArguments {
    /// `integrated` or `external`.
    pub kind: Option<String>,
    pub title: Option<String>,
    pub cwd: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, Option<String>>,
}

/// The answer to `runInTerminal`: the process that was started, when the client knows it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunInTerminalResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_process_id: Option<i64>,
}

// --- breakpoints ---

/// A file, as the adapter names it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Source {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_reference: Option<i64>,
}

impl Source {
    /// The file at `path`.
    pub fn file(path: &str) -> Self {
        Self {
            name: None,
            path: Some(path.to_owned()),
            source_reference: None,
        }
    }
}

/// A breakpoint the user set, in a file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceBreakpoint {
    pub line: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hit_condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_message: Option<String>,
}

impl SourceBreakpoint {
    pub fn at(line: i64) -> Self {
        Self {
            line,
            ..Self::default()
        }
    }
}

/// All the breakpoints of one file: what is sent replaces what the adapter had for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetBreakpointsArguments {
    pub source: Source,
    pub breakpoints: Vec<SourceBreakpoint>,
}

/// What the adapter made of a breakpoint.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Breakpoint {
    pub id: Option<i64>,
    /// Whether the adapter could place it (a comment or an empty line cannot have one).
    pub verified: bool,
    pub message: Option<String>,
    pub source: Option<Source>,
    /// Where it really is, which may not be where it was asked for.
    pub line: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct SetBreakpointsResponse {
    pub breakpoints: Vec<Breakpoint>,
}

// --- stopped: threads, the stack, variables ---

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Thread {
    pub id: i64,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ThreadsResponse {
    pub threads: Vec<Thread>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackTraceArguments {
    pub thread_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_frame: Option<i64>,
    /// How many frames; none asks for all of them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub levels: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackFrame {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub source: Option<Source>,
    #[serde(default)]
    pub line: i64,
    #[serde(default)]
    pub column: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StackTraceResponse {
    pub stack_frames: Vec<StackFrame>,
    pub total_frames: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    pub name: String,
    /// What `variables` is asked with to get its variables.
    pub variables_reference: i64,
    /// A scope that is costly to get is not got until it is opened.
    #[serde(default)]
    pub expensive: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ScopesResponse {
    pub scopes: Vec<Scope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariablesArguments {
    pub variables_reference: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<i64>,
}

impl VariablesArguments {
    /// All the children of `variables_reference`.
    pub fn of(variables_reference: i64) -> Self {
        Self {
            variables_reference,
            start: None,
            count: None,
        }
    }

    /// `count` children from the `start`-th (a long list is read in pages).
    pub fn page(variables_reference: i64, start: i64, count: i64) -> Self {
        Self {
            variables_reference,
            start: Some(start),
            count: Some(count),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variable {
    pub name: String,
    pub value: String,
    #[serde(default, rename = "type")]
    pub type_name: Option<String>,
    /// Not 0 when it has children, which `variables` gets with this number.
    #[serde(default)]
    pub variables_reference: i64,
    #[serde(default)]
    pub named_variables: Option<i64>,
    #[serde(default)]
    pub indexed_variables: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct VariablesResponse {
    pub variables: Vec<Variable>,
}

// --- running ---

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContinueResponse {
    /// Whether every thread went on; an adapter that leaves it out means all of them.
    #[serde(default = "yes")]
    pub all_threads_continued: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadArguments {
    pub thread_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopesArguments {
    pub frame_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisconnectArguments {
    /// Whether the program is ended too (when the session started it, it usually is).
    pub terminate_debuggee: bool,
}
