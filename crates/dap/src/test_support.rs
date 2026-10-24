//! A scripted debug adapter for the tests: it runs on a thread, behind a pair of in-memory
//! pipes, and plays whatever script the test gives it, so that the client (and the editor on
//! top of it) can be tried without a debugger installed.

use std::io::{self, BufReader, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{Value, json};

use crate::framing::{read_message, write_message};
use crate::{Client, Message, Rejection};

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

/// The adapter's end of the connection, as the script sees it.
pub struct Adapter {
    reader: BufReader<PipeReader>,
    writer: PipeWriter,
    seq: i64,
}

impl Adapter {
    /// The next message from the client, or `None` when it hung up.
    #[allow(
        clippy::should_implement_trait,
        reason = "it ends with None when the client hangs up, but it is not an Iterator"
    )]
    pub fn next(&mut self) -> Option<Message> {
        let value = read_message(&mut self.reader).expect("the client sent a broken message")?;
        Some(Message::from_value(value).expect("the client sent a message that is not one"))
    }

    /// Takes the next request, which must be `command`: its `seq` and arguments.
    pub fn expect_request(&mut self, command: &str) -> (i64, Value) {
        match self.next() {
            Some(Message::Request {
                seq,
                command: got,
                arguments,
            }) if got == command => (seq, arguments),
            other => panic!("expected the request {command}, got {other:?}"),
        }
    }

    /// Takes the client's answer to a request of ours: the `seq` it answers, and what it says.
    pub fn expect_response(&mut self) -> (i64, Result<Value, Rejection>) {
        match self.next() {
            Some(Message::Response {
                request_seq,
                outcome,
                ..
            }) => (request_seq, outcome),
            other => panic!("expected an answer, got {other:?}"),
        }
    }

    /// Answers the request `request_seq` well, with `body`.
    pub fn respond(&mut self, request_seq: i64, command: &str, body: Value) {
        let seq = self.next_seq();
        self.send(&Message::Response {
            seq,
            request_seq,
            command: command.to_owned(),
            outcome: Ok(body),
        });
    }

    /// Answers the request `request_seq` with a refusal.
    pub fn refuse(&mut self, request_seq: i64, command: &str, message: &str, body: Value) {
        let seq = self.next_seq();
        self.send(&Message::Response {
            seq,
            request_seq,
            command: command.to_owned(),
            outcome: Err(Rejection {
                message: message.to_owned(),
                body,
            }),
        });
    }

    /// Sends an event.
    pub fn event(&mut self, event: &str, body: Value) {
        let seq = self.next_seq();
        self.send(&Message::Event {
            seq,
            event: event.to_owned(),
            body,
        });
    }

    /// Makes a request of the client (`runInTerminal`); its `seq`, which the answer names.
    pub fn reverse_request(&mut self, command: &str, arguments: Value) -> i64 {
        let seq = self.next_seq();
        self.send(&Message::Request {
            seq,
            command: command.to_owned(),
            arguments,
        });
        seq
    }

    /// The usual start of a session: `initialize` answered with `capabilities`.
    pub fn answer_initialize(&mut self, capabilities: Value) -> Value {
        let (seq, arguments) = self.expect_request("initialize");
        self.respond(seq, "initialize", capabilities);
        arguments
    }

    /// Sends bytes as they are, to try what the client does with a stream that is broken.
    pub fn send_raw(&mut self, bytes: &[u8]) {
        let _ = self.writer.write_all(bytes);
    }

    fn next_seq(&mut self) -> i64 {
        self.seq += 1;
        self.seq
    }

    fn send(&mut self, message: &Message) {
        // A client that has gone is not the script's concern.
        let _ = write_message(&mut self.writer, &message.to_value());
    }
}

/// A scripted adapter that is running, and how often the client was woken.
pub struct Fake {
    thread: Option<JoinHandle<()>>,
    wakes: Arc<AtomicUsize>,
}

impl Fake {
    /// How many times the client was told that something came.
    pub fn wakes(&self) -> usize {
        self.wakes.load(Ordering::SeqCst)
    }

    /// Waits for the script to end. A script that panicked (an expected message that did not
    /// come) makes this panic, so that a test sees it.
    pub fn finish(mut self) {
        if let Some(thread) = self.thread.take() {
            thread.join().expect("the scripted adapter panicked");
        }
    }
}

/// Starts an adapter that plays `script`, and a client connected to it. The script ends when it
/// returns, which hangs up on the client.
pub fn fake_adapter(script: impl FnOnce(&mut Adapter) + Send + 'static) -> (Client, Fake) {
    let (client_reads, adapter_writes) = pipe();
    let (adapter_reads, client_writes) = pipe();
    let wakes = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&wakes);
    let client = Client::new(client_reads, client_writes, move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    let thread = std::thread::spawn(move || {
        let mut adapter = Adapter {
            reader: BufReader::new(adapter_reads),
            writer: adapter_writes,
            seq: 0,
        };
        script(&mut adapter);
    });
    let fake = Fake {
        thread: Some(thread),
        wakes,
    };
    (client, fake)
}

/// A `stopped` event for the tests.
pub fn stopped(reason: &str, thread_id: i64) -> Value {
    json!({ "reason": reason, "threadId": thread_id, "allThreadsStopped": true })
}
