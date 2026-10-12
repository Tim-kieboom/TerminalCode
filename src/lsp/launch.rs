//! Starting a server: running the program, the handshake, and what went wrong
//! when it did not work.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::Value;

use super::{Connection, Incoming, LspError, Server, initialize};

/// How long a server may take to answer `initialize`. Indexing a project comes
/// after it and is reported as progress, so this is only the start of the program.
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(30);
/// The lines at the start of a server's error output that are kept, for saying why it died.
const STDERR_LINES: usize = 20;

/// What to run, and for which project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LaunchSpec {
    /// The name the server has in the settings (`rust`).
    pub(crate) name: String,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    /// The project root: the folder the program runs in and is told about.
    pub(crate) root: PathBuf,
    pub(crate) initialization_options: Value,
}

/// A session with a server that has been started and initialized.
pub(crate) struct Session {
    pub(crate) connection: Connection,
    /// What the server sends for the app, and the end of the session.
    pub(crate) incoming: Receiver<Incoming>,
    pub(crate) server: Server,
    /// Keeps the program alive; dropping it ends the program.
    #[allow(dead_code, reason = "held for its Drop")]
    guard: Box<dyn Send>,
}

impl Session {
    pub(crate) fn new(
        connection: Connection,
        incoming: Receiver<Incoming>,
        server: Server,
        guard: Box<dyn Send>,
    ) -> Self {
        Self {
            connection,
            incoming,
            server,
            guard,
        }
    }
}

/// Why a server could not be started.
#[derive(Debug)]
pub(crate) enum StartError {
    /// The program could not be run at all.
    Spawn(String),
    /// It ran, but the handshake did not work: it exited, did not answer, or
    /// refused. The first line it wrote to its error output is kept, which is
    /// where a program says why (the `rustup` shim for a missing component
    /// prints "Unknown binary" and exits).
    Handshake {
        error: LspError,
        stderr: Option<String>,
    },
}

impl StartError {
    /// One line for the user.
    pub(crate) fn reason(&self) -> String {
        match self {
            Self::Spawn(reason) => reason.clone(),
            Self::Handshake {
                error,
                stderr: Some(line),
            } => format!("{error} ({line})"),
            Self::Handshake {
                error,
                stderr: None,
            } => error.to_string(),
        }
    }
}

/// Something that can start servers. The real one runs programs; the tests
/// bring their own.
pub(crate) trait Launcher: Send + Sync + 'static {
    /// Starts the server and finishes the handshake. `wake` is called when the
    /// session has something for the app.
    fn launch(
        &self,
        spec: &LaunchSpec,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Session, StartError>;
}

/// Runs the server as a child process, talking over its standard input and output.
#[derive(Debug, Default)]
pub(crate) struct ProcessLauncher;

impl Launcher for ProcessLauncher {
    fn launch(
        &self,
        spec: &LaunchSpec,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Session, StartError> {
        let mut command = Command::new(&spec.command);
        command
            .args(&spec.args)
            .current_dir(&spec.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        hide_console_window(&mut command);

        let mut child = command.spawn().map_err(|error| {
            StartError::Spawn(match error.kind() {
                std::io::ErrorKind::NotFound => {
                    format!(
                        "`{}` was not found (is it installed and on your PATH?)",
                        spec.command
                    )
                }
                _ => format!("cannot run `{}`: {error}", spec.command),
            })
        })?;
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err(StartError::Spawn(format!(
                "`{}` did not give its input and output",
                spec.command
            )));
        };

        let tail = ErrorTail::start(stderr);
        let (connection, incoming) = Connection::start(stdout, stdin, move || wake());
        let started = initialize(
            &connection,
            &spec.root,
            &spec.initialization_options,
            INITIALIZE_TIMEOUT,
        );
        let guard = ProcessGuard { child, tail };
        match started {
            Ok(server) => Ok(Session::new(connection, incoming, server, Box::new(guard))),
            Err(error) => Err(StartError::Handshake {
                stderr: guard.stop_and_first_error_line(),
                error,
            }),
        }
    }
}

/// A server must not open a console window of its own on Windows.
fn hide_console_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

/// The child process; dropping it ends it.
struct ProcessGuard {
    child: Child,
    tail: ErrorTail,
}

impl ProcessGuard {
    /// Ends the process and says what it wrote first to its error output.
    fn stop_and_first_error_line(mut self) -> Option<String> {
        // Only when it has not exited by itself does it need killing.
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.tail.first_line()
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The start of what a program wrote to its error output, read on a thread so
/// that a program that writes a lot never blocks on a full pipe.
struct ErrorTail {
    lines: Arc<Mutex<VecDeque<String>>>,
    reader: Option<JoinHandle<()>>,
}

impl ErrorTail {
    fn start(stderr: impl std::io::Read + Send + 'static) -> Self {
        let lines = Arc::new(Mutex::new(VecDeque::new()));
        let kept = Arc::clone(&lines);
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let mut kept = kept
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                // Only the start is kept, but the pipe is read to its end.
                if kept.len() < STDERR_LINES {
                    kept.push_back(line);
                }
            }
        });
        Self {
            lines,
            reader: Some(reader),
        }
    }

    /// The first line with something in it. Waits for the program's error
    /// output to end, which it has once the program is gone.
    fn first_line(&mut self) -> Option<String> {
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        let lines = self
            .lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        lines
            .iter()
            .map(|line| line.trim())
            .find(|line| !line.is_empty())
            .map(str::to_owned)
    }
}
