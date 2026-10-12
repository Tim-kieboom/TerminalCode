//! A scripted language server for the tests: it runs on a thread, behind a pair
//! of in-memory pipes, and plays whatever script the test gives it.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{Value, json};

use crate::lsp::framing::{read_message, write_message};
use crate::lsp::{Connection, Id, Incoming, Message, RpcError};

/// How long a test waits for something that should happen at once.
pub(super) const PATIENCE: Duration = Duration::from_secs(10);

/// The writing end of an in-memory pipe.
pub(super) struct PipeWriter(Sender<Vec<u8>>);

/// The reading end: ends (reads zero bytes) once the writer is dropped.
pub(super) struct PipeReader {
    chunks: Receiver<Vec<u8>>,
    current: Vec<u8>,
    taken: usize,
}

pub(super) fn pipe() -> (PipeReader, PipeWriter) {
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
pub(super) struct Session {
    reader: BufReader<PipeReader>,
    writer: PipeWriter,
}

impl Session {
    /// The next message from the client, or `None` when it hung up.
    pub(super) fn next(&mut self) -> Option<Message> {
        let value = read_message(&mut self.reader).expect("the client sent a broken message")?;
        Some(Message::from_value(value).expect("the client sent a message that is not one"))
    }

    pub(super) fn expect_request(&mut self, method: &str) -> (Id, Value) {
        match self.next() {
            Some(Message::Request {
                id,
                method: got,
                params,
            }) if got == method => (id, params),
            other => panic!("expected the request {method}, got {other:?}"),
        }
    }

    pub(super) fn expect_notification(&mut self, method: &str) -> Value {
        match self.next() {
            Some(Message::Notification {
                method: got,
                params,
            }) if got == method => params,
            other => panic!("expected the notification {method}, got {other:?}"),
        }
    }

    pub(super) fn expect_response(&mut self) -> (Id, Result<Value, RpcError>) {
        match self.next() {
            Some(Message::Response { id, outcome }) => (id, outcome),
            other => panic!("expected an answer, got {other:?}"),
        }
    }

    pub(super) fn respond(&mut self, id: Id, result: Value) {
        self.send(&Message::Response {
            id,
            outcome: Ok(result),
        });
    }

    pub(super) fn respond_error(&mut self, id: Id, code: i64, message: &str) {
        self.send(&Message::Response {
            id,
            outcome: Err(RpcError {
                code,
                message: message.to_owned(),
                data: None,
            }),
        });
    }

    pub(super) fn notify(&mut self, method: &str, params: Value) {
        self.send(&Message::Notification {
            method: method.to_owned(),
            params,
        });
    }

    /// Asks the client for something, as a server does.
    pub(super) fn request(&mut self, id: i64, method: &str, params: Value) {
        self.send(&Message::Request {
            id: Id::Number(id),
            method: method.to_owned(),
            params,
        });
    }

    pub(super) fn send(&mut self, message: &Message) {
        write_message(&mut self.writer, &message.to_value()).expect("the client is gone");
    }

    /// Bytes as they are, for a stream that is not well formed.
    pub(super) fn send_raw(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).expect("the client is gone");
    }

    /// Answers `initialize` as a server that can do what the tests need, and
    /// reads the `initialized` that follows.
    pub(super) fn start_up(&mut self) -> Value {
        let (id, params) = self.expect_request("initialize");
        self.respond(
            id,
            json!({
                "capabilities": { "textDocumentSync": 2 },
                "serverInfo": { "name": "fake-server", "version": "1.0" },
            }),
        );
        self.expect_notification("initialized");
        params
    }
}

/// The server thread. A script that fails an expectation fails the test when
/// [`FakeServer::finish`] joins it.
pub(super) struct FakeServer {
    handle: JoinHandle<()>,
}

impl FakeServer {
    /// Joins the script, passing on a panic from it.
    pub(super) fn finish(self) {
        if let Err(panic) = self.handle.join() {
            std::panic::resume_unwind(panic);
        }
    }
}

/// A connection to a fake server that plays `script`, with what arrives from
/// it for the app.
pub(super) struct Rig {
    pub(super) connection: Connection,
    pub(super) incoming: Receiver<Incoming>,
    pub(super) server: FakeServer,
}

pub(super) fn connect(script: impl FnOnce(&mut Session) + Send + 'static) -> Rig {
    connect_with_wake(script, || {})
}

pub(super) fn connect_with_wake(
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
    pub(super) fn next_incoming(&self) -> Incoming {
        self.incoming
            .recv_timeout(PATIENCE)
            .expect("nothing came from the server")
    }
}

/// A `BufRead` over bytes, for the framing tests.
pub(super) fn reader(bytes: &[u8]) -> impl BufRead + '_ {
    BufReader::new(bytes)
}
