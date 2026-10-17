//! A scripted language server for the tests: it runs on a thread, behind a pair
//! of in-memory pipes, and plays whatever script the test gives it.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{Value, json};

use crate::framing::{read_message, write_message};
use crate::{Connection, Id, Incoming, Message, RpcError};

/// How long a test waits for something that should happen at once.
pub const PATIENCE: Duration = Duration::from_secs(10);

/// The writing end of an in-memory pipe.
pub struct PipeWriter(Sender<Vec<u8>>);

/// The reading end: ends (reads zero bytes) once the writer is dropped.
pub struct PipeReader {
    chunks: Receiver<Vec<u8>>,
    current: Vec<u8>,
    taken: usize,
}

pub fn pipe() -> (PipeReader, PipeWriter) {
    let (sender, chunks) = mpsc::channel();
    let reader = PipeReader {
        chunks,
        current: Vec::new(),
        taken: 0,
    };
    (reader, PipeWriter(sender))
}

impl Write for PipeWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .send(bytes.to_vec())
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "the reader is gone"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Read for PipeReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        while self.taken == self.current.len() {
            match self.chunks.recv() {
                Ok(chunk) => (self.current, self.taken) = (chunk, 0),
                Err(_) => return Ok(0),
            }
        }
        let count = buffer.len().min(self.current.len() - self.taken);
        buffer[..count].copy_from_slice(&self.current[self.taken..self.taken + count]);
        self.taken += count;
        Ok(count)
    }
}

/// The server's end of the connection, as the script sees it.
pub struct Session {
    reader: BufReader<PipeReader>,
    writer: PipeWriter,
}

impl Session {
    /// The next message from the client, or `None` when it hung up.
    #[allow(
        clippy::should_implement_trait,
        reason = "it ends with None when the client hangs up, but it is not an Iterator"
    )]
    pub fn next(&mut self) -> Option<Message> {
        let value = read_message(&mut self.reader).expect("the client sent a broken message")?;
        Some(Message::from_value(value).expect("the client sent a message that is not one"))
    }

    pub fn expect_request(&mut self, method: &str) -> (Id, Value) {
        match self.next() {
            Some(Message::Request {
                id,
                method: got,
                params,
            }) if got == method => (id, params),
            other => panic!("expected the request {method}, got {other:?}"),
        }
    }

    pub fn expect_notification(&mut self, method: &str) -> Value {
        match self.next() {
            Some(Message::Notification {
                method: got,
                params,
            }) if got == method => params,
            other => panic!("expected the notification {method}, got {other:?}"),
        }
    }

    pub fn expect_response(&mut self) -> (Id, Result<Value, RpcError>) {
        match self.next() {
            Some(Message::Response { id, outcome }) => (id, outcome),
            other => panic!("expected an answer, got {other:?}"),
        }
    }

    pub fn respond(&mut self, id: Id, result: Value) {
        self.send(&Message::Response {
            id,
            outcome: Ok(result),
        });
    }

    pub fn respond_error(&mut self, id: Id, code: i64, message: &str) {
        self.send(&Message::Response {
            id,
            outcome: Err(RpcError {
                code,
                message: message.to_owned(),
                data: None,
            }),
        });
    }

    pub fn notify(&mut self, method: &str, params: Value) {
        self.send(&Message::Notification {
            method: method.to_owned(),
            params,
        });
    }

    /// Asks the client for something, as a server does.
    pub fn request(&mut self, id: i64, method: &str, params: Value) {
        self.send(&Message::Request {
            id: Id::Number(id),
            method: method.to_owned(),
            params,
        });
    }

    pub fn send(&mut self, message: &Message) {
        write_message(&mut self.writer, &message.to_value()).expect("the client is gone");
    }

    /// Bytes as they are, for a stream that is not well formed.
    pub fn send_raw(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).expect("the client is gone");
    }

    /// Answers `initialize` as a server that can do what the tests need, and
    /// reads the `initialized` that follows.
    pub fn start_up(&mut self) -> Value {
        self.start_up_with(json!({ "textDocumentSync": 2 }))
    }

    /// The same, for a server that says it can do what `capabilities` says.
    pub fn start_up_with(&mut self, capabilities: Value) -> Value {
        let (id, params) = self.expect_request("initialize");
        self.respond(
            id,
            json!({
                "capabilities": capabilities,
                "serverInfo": { "name": "fake-server", "version": "1.0" },
            }),
        );
        self.expect_notification("initialized");
        params
    }
}

/// The server thread. A script that fails an expectation fails the test when
/// [`FakeServer::finish`] joins it.
pub struct FakeServer {
    handle: JoinHandle<()>,
}

impl FakeServer {
    /// Joins the script, passing on a panic from it.
    pub fn finish(self) {
        if let Err(panic) = self.handle.join() {
            std::panic::resume_unwind(panic);
        }
    }
}

/// A connection to a fake server that plays `script`, with what arrives from
/// it for the app.
pub struct Rig {
    pub connection: Connection,
    pub incoming: Receiver<Incoming>,
    pub server: FakeServer,
}

pub fn connect(script: impl FnOnce(&mut Session) + Send + 'static) -> Rig {
    connect_with_wake(script, || {})
}

pub fn connect_with_wake(
    script: impl FnOnce(&mut Session) + Send + 'static,
    wake: impl Fn() + Send + 'static,
) -> Rig {
    let (client_reader, server_writer) = pipe();
    let (server_reader, client_writer) = pipe();
    let handle = std::thread::spawn(move || {
        let mut session = Session {
            reader: BufReader::new(server_reader),
            writer: server_writer,
        };
        script(&mut session);
    });
    let (connection, incoming) = Connection::start(client_reader, client_writer, wake);
    Rig {
        connection,
        incoming,
        server: FakeServer { handle },
    }
}

impl Rig {
    pub fn next_incoming(&self) -> Incoming {
        self.incoming
            .recv_timeout(PATIENCE)
            .expect("nothing came from the server")
    }
}

/// A `BufRead` over bytes, for the framing tests.
pub fn reader(bytes: &[u8]) -> impl BufRead + '_ {
    BufReader::new(bytes)
}

// ---- a launcher that starts fake servers

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{LaunchSpec, Launcher, Session as LspSession, StartError, initialize};

/// What a started fake server does after it has answered `initialize`.
pub type Script = Box<dyn FnOnce(&mut Session) + Send>;

/// Starts a fake server for each launch, running the script that `script`
/// gives for that launch (counted from zero); `None` is a program that cannot
/// be run. A script that returns is a server that died.
pub struct FakeLauncher {
    pub launches: Arc<AtomicUsize>,
    /// What each launch was asked to run.
    specs: std::sync::Mutex<Vec<LaunchSpec>>,
    /// What the servers say they can do.
    capabilities: Value,
    script: Box<dyn Fn(usize) -> Option<Script> + Send + Sync>,
}

impl FakeLauncher {
    pub fn new(script: impl Fn(usize) -> Option<Script> + Send + Sync + 'static) -> Arc<Self> {
        Self::with_capabilities(json!({ "textDocumentSync": 2 }), script)
    }

    /// Servers that say they can do what `capabilities` says.
    pub fn with_capabilities(
        capabilities: Value,
        script: impl Fn(usize) -> Option<Script> + Send + Sync + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            launches: Arc::new(AtomicUsize::new(0)),
            specs: std::sync::Mutex::new(Vec::new()),
            capabilities,
            script: Box::new(script),
        })
    }

    pub fn launched(&self) -> usize {
        self.launches.load(Ordering::SeqCst)
    }

    pub fn specs(&self) -> Vec<LaunchSpec> {
        self.specs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl Launcher for FakeLauncher {
    fn launch(
        &self,
        spec: &LaunchSpec,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<LspSession, StartError> {
        let number = self.launches.fetch_add(1, Ordering::SeqCst);
        self.specs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(spec.clone());
        let Some(script) = (self.script)(number) else {
            return Err(StartError::Spawn(format!(
                "`{}` was not found",
                spec.command
            )));
        };
        let capabilities = self.capabilities.clone();
        let rig = connect_with_wake(
            move |session| {
                session.start_up_with(capabilities);
                script(session);
            },
            move || wake(),
        );
        match initialize(
            &rig.connection,
            &spec.root,
            &spec.initialization_options,
            PATIENCE,
        ) {
            Ok(server) => Ok(LspSession::new(
                rig.connection,
                rig.incoming,
                server,
                Box::new(rig.server),
            )),
            Err(error) => Err(StartError::Handshake {
                error,
                stderr: Some("it said so on stderr".to_owned()),
            }),
        }
    }
}

/// A server that stays until the client hangs up.
pub fn stays() -> Option<Script> {
    Some(Box::new(|session| while session.next().is_some() {}))
}

/// A server that answers `initialize` and then dies.
pub fn dies() -> Option<Script> {
    Some(Box::new(|_| {}))
}

/// A server that passes on everything it is sent, and stays until the client
/// hangs up.
pub fn recording(sent: std::sync::mpsc::Sender<crate::Message>) -> Option<Script> {
    Some(Box::new(move |session| {
        while let Some(message) = session.next() {
            let _ = sent.send(message);
        }
    }))
}

/// A server that passes on what it is sent until it has had `count` messages,
/// and then dies.
pub fn recording_then_dying(
    sent: std::sync::mpsc::Sender<crate::Message>,
    count: usize,
) -> Option<Script> {
    Some(Box::new(move |session| {
        for _ in 0..count {
            match session.next() {
                Some(message) => {
                    let _ = sent.send(message);
                }
                None => return,
            }
        }
    }))
}
