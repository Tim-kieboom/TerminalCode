//! The client: typed requests to an adapter, and what comes back from it as [`Update`]s.
//!
//! A session goes like this (the order the protocol asks for; the editor's state machine in
//! `app/` takes the steps, this only offers them):
//!
//! 1. [`Client::initialize`]: the answer is what the adapter can do.
//! 2. [`Client::launch`] with the arguments of the adapter. Some adapters answer it at once,
//!    others only after `configurationDone`, so it is not waited for yet.
//! 3. The event [`Event::Initialized`]: the adapter is ready for the breakpoints, one
//!    [`Client::set_breakpoints`] for each file, then [`Client::configuration_done`].
//! 4. The adapter may ask to start the program in a terminal ([`ReverseRequest::RunInTerminal`]),
//!    answered with [`Client::respond_run_in_terminal`].
//! 5. [`Event::Stopped`], and while stopped [`Client::threads`], [`Client::stack_trace`],
//!    [`Client::scopes`] and [`Client::variables`]; [`Client::continue_`] and the steps go on.
//! 6. [`Event::Exited`] and [`Event::Terminated`]; [`Client::disconnect`] ends it.

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::DapError;
use crate::connection::{Connection, Incoming, PendingReply};
use crate::message::Rejection;
use crate::types::*;

/// Something the adapter did that is for the editor.
#[derive(Debug, Clone, PartialEq)]
pub enum Update {
    Event(Event),
    /// The adapter asks the client for something; answer it with the method for the request.
    Reverse {
        seq: i64,
        request: ReverseRequest,
    },
    /// The adapter is gone, and why. The last update.
    Closed(String),
}

/// A session with one adapter. Dropping it ends the connection and, when the adapter was
/// started by [`Client::spawn`], the adapter's process.
pub struct Client {
    connection: Connection,
    incoming: Receiver<Incoming>,
    process: Option<Child>,
}

impl Client {
    /// A client over the streams of an adapter that is running already (a test's, or a
    /// connection that was made some other way). `wake` is called when something arrives.
    pub fn new(
        reader: impl Read + Send + 'static,
        writer: impl Write + Send + 'static,
        wake: impl Fn() + Send + 'static,
    ) -> Self {
        let (connection, incoming) = Connection::start(reader, writer, wake);
        Self {
            connection,
            incoming,
            process: None,
        }
    }

    /// Starts the adapter `program` with `args` and talks to it over its standard input and
    /// output. What it writes to its standard error is dropped, unless `TRCODE_DEBUG_LOG` names a
    /// file: then that file gets everything sent and received (`>>` and `<<`) and the adapter's
    /// standard error, to find out why an adapter does not do what it should.
    pub fn spawn(
        program: &str,
        args: &[String],
        wake: impl Fn() + Send + 'static,
    ) -> Result<Self, DapError> {
        let spawn_error = |source| DapError::Spawn {
            program: program.to_owned(),
            source,
        };
        let log = std::env::var_os("TRCODE_DEBUG_LOG").and_then(|path| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .ok()
        });
        let stderr = log
            .as_ref()
            .and_then(|file| file.try_clone().ok())
            .map_or_else(Stdio::null, Stdio::from);
        // An adapter found by name tells the launcher it asks for as its own name, which is then
        // taken for a file of the project's folder: it is started by its whole path.
        let found = find_program(program);
        let mut child = Command::new(&found)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr)
            .spawn()
            .map_err(spawn_error)?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err(spawn_error(std::io::Error::other(
                "no pipes to the adapter",
            )));
        };
        let mut client = match log {
            Some(file) => {
                let reader = Tee::new(stdout, file.try_clone().ok(), "<<");
                let writer = Tee::new(stdin, Some(file), ">>");
                Self::new(reader, writer, wake)
            }
            None => Self::new(stdout, stdin, wake),
        };
        client.process = Some(child);
        Ok(client)
    }

    // --- starting ---

    pub fn initialize(
        &self,
        arguments: &InitializeArguments,
    ) -> Result<PendingReply<Capabilities>, DapError> {
        self.request("initialize", arguments)
    }

    /// Starts the program under the adapter. `arguments` are the adapter's own (`program`,
    /// `args`, `cwd`, ... for `lldb-dap`).
    pub fn launch(&self, arguments: Value) -> Result<PendingReply<Nothing>, DapError> {
        self.connection.request("launch", arguments)
    }

    /// Ends the setup: the breakpoints are sent, the program may run.
    pub fn configuration_done(&self) -> Result<PendingReply<Nothing>, DapError> {
        self.connection.request("configurationDone", Value::Null)
    }

    /// Ends the session. With `terminate_debuggee` the program is ended too.
    pub fn disconnect(&self, terminate_debuggee: bool) -> Result<PendingReply<Nothing>, DapError> {
        self.request("disconnect", &DisconnectArguments { terminate_debuggee })
    }

    /// Asks the program to end (when the adapter supports it, see [`Capabilities`]).
    pub fn terminate(&self) -> Result<PendingReply<Nothing>, DapError> {
        self.connection.request("terminate", Value::Null)
    }

    // --- breakpoints ---

    /// Sets all the breakpoints of one file: what the adapter had for it is replaced.
    pub fn set_breakpoints(
        &self,
        arguments: &SetBreakpointsArguments,
    ) -> Result<PendingReply<SetBreakpointsResponse>, DapError> {
        self.request("setBreakpoints", arguments)
    }

    // --- running ---

    pub fn continue_(&self, thread_id: i64) -> Result<PendingReply<ContinueResponse>, DapError> {
        self.request("continue", &ThreadArguments { thread_id })
    }

    /// Step over.
    pub fn next(&self, thread_id: i64) -> Result<PendingReply<Nothing>, DapError> {
        self.request("next", &ThreadArguments { thread_id })
    }

    pub fn step_in(&self, thread_id: i64) -> Result<PendingReply<Nothing>, DapError> {
        self.request("stepIn", &ThreadArguments { thread_id })
    }

    pub fn step_out(&self, thread_id: i64) -> Result<PendingReply<Nothing>, DapError> {
        self.request("stepOut", &ThreadArguments { thread_id })
    }

    pub fn pause(&self, thread_id: i64) -> Result<PendingReply<Nothing>, DapError> {
        self.request("pause", &ThreadArguments { thread_id })
    }

    // --- stopped ---

    pub fn threads(&self) -> Result<PendingReply<ThreadsResponse>, DapError> {
        self.connection.request("threads", Value::Null)
    }

    pub fn stack_trace(
        &self,
        arguments: &StackTraceArguments,
    ) -> Result<PendingReply<StackTraceResponse>, DapError> {
        self.request("stackTrace", arguments)
    }

    pub fn scopes(&self, frame_id: i64) -> Result<PendingReply<ScopesResponse>, DapError> {
        self.request("scopes", &ScopesArguments { frame_id })
    }

    pub fn variables(
        &self,
        arguments: &VariablesArguments,
    ) -> Result<PendingReply<VariablesResponse>, DapError> {
        self.request("variables", arguments)
    }

    /// Answers `runInTerminal`: the process that was started, or why it could not be.
    pub fn respond_run_in_terminal(
        &self,
        request_seq: i64,
        outcome: Result<RunInTerminalResponse, String>,
    ) -> Result<(), DapError> {
        let outcome = outcome
            .map(|response| serde_json::to_value(response).unwrap_or(Value::Null))
            .map_err(Rejection::new);
        self.connection
            .respond(request_seq, "runInTerminal", outcome)
    }

    // --- what comes in ---

    /// The next thing the adapter did, if something is waiting.
    pub fn next_update(&self) -> Option<Update> {
        loop {
            match self.incoming.try_recv() {
                Ok(incoming) => {
                    if let Some(update) = self.convert(incoming) {
                        return Some(update);
                    }
                }
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => return None,
            }
        }
    }

    /// The next thing the adapter does, waiting at most `timeout` for it.
    pub fn wait_update(&self, timeout: Duration) -> Option<Update> {
        let until = Instant::now() + timeout;
        loop {
            let left = until.checked_duration_since(Instant::now())?;
            match self.incoming.recv_timeout(left) {
                Ok(incoming) => {
                    if let Some(update) = self.convert(incoming) {
                        return Some(update);
                    }
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return None,
            }
        }
    }

    /// An update for what the adapter said; `None` for a request of the adapter that this
    /// client does not do, which is answered (refused) here so that the adapter is never left
    /// waiting.
    fn convert(&self, incoming: Incoming) -> Option<Update> {
        match incoming {
            Incoming::Event { event, body } => Some(Update::Event(Event::parse(&event, body))),
            Incoming::Closed(reason) => Some(Update::Closed(reason)),
            Incoming::Request {
                seq,
                command,
                arguments,
            } => {
                let parsed = match command.as_str() {
                    "runInTerminal" => serde_json::from_value(arguments)
                        .ok()
                        .map(ReverseRequest::RunInTerminal),
                    _ => None,
                };
                match parsed {
                    Some(request) => Some(Update::Reverse { seq, request }),
                    None => {
                        let refusal = Rejection::new("unsupported");
                        let _ = self.connection.respond(seq, &command, Err(refusal));
                        None
                    }
                }
            }
        }
    }

    fn request<T: DeserializeOwned>(
        &self,
        command: &str,
        arguments: &impl Serialize,
    ) -> Result<PendingReply<T>, DapError> {
        let arguments = serde_json::to_value(arguments).map_err(|error| DapError::Unexpected {
            command: command.to_owned(),
            detail: error.to_string(),
        })?;
        self.connection.request(command, arguments)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // The connection is dropped after this, which closes the adapter's input; an adapter
        // that does not leave by itself is ended.
        if let Some(mut child) = self.process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// A reader or writer that also copies what passes to a log file.
struct Tee<T> {
    inner: T,
    log: Option<std::fs::File>,
    mark: &'static str,
}

impl<T> Tee<T> {
    fn new(inner: T, log: Option<std::fs::File>, mark: &'static str) -> Self {
        Self { inner, log, mark }
    }

    fn note(&mut self, bytes: &[u8]) {
        use std::io::Write as _;
        if let Some(log) = &mut self.log {
            let _ = writeln!(log, "{} {}", self.mark, String::from_utf8_lossy(bytes));
        }
    }
}

impl<T: Read> Read for Tee<T> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = self.inner.read(buffer)?;
        self.note(&buffer[..count]);
        Ok(count)
    }
}

impl<T: Write> Write for Tee<T> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let count = self.inner.write(buffer)?;
        self.note(&buffer[..count]);
        Ok(count)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// The whole path of `program` when it is a name to be found on the `PATH`; anything with a
/// separator in it, or not found, as it is.
pub(crate) fn find_program(program: &str) -> std::path::PathBuf {
    let name = std::path::Path::new(program);
    if name.components().count() != 1 {
        return name.to_path_buf();
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return name.to_path_buf();
    };
    std::env::split_paths(&paths)
        .map(|folder| folder.join(name))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| name.to_path_buf())
}
