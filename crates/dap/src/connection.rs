//! One session with an adapter over a pair of byte streams: requests that wait for their
//! answers, events and reverse requests coming in, and a thread that reads.

use std::collections::HashMap;
use std::io::{self, BufReader, Read, Write};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::DapError;
use crate::framing::{read_message, write_message};
use crate::message::{Message, Rejection};

/// What the adapter said that is not the answer to a request.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Event {
        event: String,
        body: Value,
    },
    /// The adapter asks the client to do something (`runInTerminal`, `startDebugging`): it
    /// is answered with [`Connection::respond`], and the adapter waits until then.
    Request {
        seq: i64,
        command: String,
        arguments: Value,
    },
    /// The connection is over, and why. Always the last thing sent.
    Closed(String),
}

type Reply = Result<Value, Rejection>;
/// Shared by the app and the reading thread. `None` once the session is closed from our side.
type Writer = Arc<Mutex<Option<Box<dyn Write + Send>>>>;

/// The requests that have not been answered yet, and whether they ever will be.
#[derive(Default)]
struct Pending {
    waiting: HashMap<i64, Sender<Result<Reply, DapError>>>,
    /// Set when the connection ended: why.
    closed: Option<String>,
}

type SharedPending = Arc<Mutex<Pending>>;

/// A session with one adapter. Dropping it closes what we write to, which tells an adapter on
/// the other end of a pipe that nothing more will come; the reading thread ends when the adapter
/// closes its side.
pub struct Connection {
    writer: Writer,
    pending: SharedPending,
    next_seq: AtomicI64,
}

/// A request that was sent and has not been answered yet; the answer is read as a `T`.
pub struct PendingReply<T> {
    command: String,
    seq: i64,
    answer: Receiver<Result<Reply, DapError>>,
    pending: SharedPending,
    kind: PhantomData<fn() -> T>,
}

impl Connection {
    /// Starts reading `reader` on a thread of its own. What the adapter sends that is not an
    /// answer arrives on the returned channel, and `wake` is called after each such message
    /// (and each answer), so a loop that waits on something else can look.
    pub fn start(
        reader: impl Read + Send + 'static,
        writer: impl Write + Send + 'static,
        wake: impl Fn() + Send + 'static,
    ) -> (Self, Receiver<Incoming>) {
        let writer: Writer = Arc::new(Mutex::new(Some(Box::new(writer))));
        let pending = SharedPending::default();
        let (incoming, received) = mpsc::channel();
        let reading = Reading {
            pending: Arc::clone(&pending),
            incoming,
        };
        std::thread::spawn(move || reading.run(reader, &wake));
        let connection = Self {
            writer,
            pending,
            next_seq: AtomicI64::new(1),
        };
        (connection, received)
    }

    /// Sends a request. Wait for the answer with [`PendingReply::wait`].
    pub fn request<T: DeserializeOwned>(
        &self,
        command: &str,
        arguments: Value,
    ) -> Result<PendingReply<T>, DapError> {
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        let (sender, answer) = mpsc::channel();
        // Registered before it is sent: the answer can be quicker than this thread.
        {
            let mut pending = lock(&self.pending);
            if let Some(reason) = &pending.closed {
                return Err(DapError::Closed(reason.clone()));
            }
            pending.waiting.insert(seq, sender);
        }
        let message = Message::Request {
            seq,
            command: command.to_owned(),
            arguments,
        };
        if let Err(error) = self.send(&message) {
            lock(&self.pending).waiting.remove(&seq);
            return Err(error);
        }
        Ok(PendingReply {
            command: command.to_owned(),
            seq,
            answer,
            pending: Arc::clone(&self.pending),
            kind: PhantomData,
        })
    }

    /// Answers a request the adapter made, which [`Incoming::Request`] handed over.
    pub fn respond(
        &self,
        request_seq: i64,
        command: &str,
        outcome: Result<Value, Rejection>,
    ) -> Result<(), DapError> {
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        self.send(&Message::Response {
            seq,
            request_seq,
            command: command.to_owned(),
            outcome,
        })
    }

    /// A write that fails means the session is over (an adapter that exits at once makes the
    /// first write fail before the reading thread has noticed the end), so from then on every
    /// request fails at once.
    fn send(&self, message: &Message) -> Result<(), DapError> {
        let written = match lock(&self.writer).as_mut() {
            Some(writer) => write_message(writer, &message.to_value()),
            None => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the session was closed",
            )),
        };
        written.map_err(|error| {
            let reason = format!("could not send to the debug adapter: {error}");
            let mut pending = lock(&self.pending);
            let reason = pending.closed.get_or_insert(reason).clone();
            DapError::Closed(reason)
        })
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        lock(&self.writer).take();
    }
}

impl<T: DeserializeOwned> PendingReply<T> {
    /// The adapter's answer, waiting at most `timeout` for it. An answer that comes after that
    /// is dropped.
    pub fn wait(self, timeout: Duration) -> Result<T, DapError> {
        let outcome = match self.answer.recv_timeout(timeout) {
            Ok(outcome) => outcome,
            Err(RecvTimeoutError::Timeout) => {
                return Err(DapError::Timeout {
                    command: self.command.clone(),
                    seconds: timeout.as_secs(),
                });
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(DapError::Closed("the connection ended".to_owned()));
            }
        };
        self.finish(outcome)
    }

    /// The answer if it has come, without waiting: `None` while it has not.
    pub fn try_take(&mut self) -> Option<Result<T, DapError>> {
        let outcome = match self.answer.try_recv() {
            Ok(outcome) => outcome,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => {
                return Some(Err(DapError::Closed("the connection ended".to_owned())));
            }
        };
        Some(self.finish(outcome))
    }

    fn finish(&self, outcome: Result<Reply, DapError>) -> Result<T, DapError> {
        let body = match outcome? {
            Ok(body) => body,
            Err(rejection) => {
                return Err(DapError::Rejected {
                    command: self.command.clone(),
                    rejection,
                });
            }
        };
        serde_json::from_value(body).map_err(|error| DapError::Unexpected {
            command: self.command.clone(),
            detail: error.to_string(),
        })
    }
}

/// A request nobody waits for any more (it timed out, or its answer is no longer wanted) must
/// not stay in the table of requests that wait.
impl<T> Drop for PendingReply<T> {
    fn drop(&mut self) {
        lock(&self.pending).waiting.remove(&self.seq);
    }
}

/// What the reading thread holds.
struct Reading {
    pending: SharedPending,
    incoming: Sender<Incoming>,
}

impl Reading {
    fn run(self, reader: impl Read, wake: &dyn Fn()) {
        let mut reader = BufReader::new(reader);
        let reason = loop {
            let value = match read_message(&mut reader) {
                Ok(Some(value)) => value,
                Ok(None) => break "the adapter ended the connection".to_owned(),
                Err(error) => break error.to_string(),
            };
            // A message that is not one is skipped: one odd message is not a reason to lose
            // the session.
            let Ok(message) = Message::from_value(value) else {
                continue;
            };
            match message {
                Message::Response {
                    request_seq,
                    outcome,
                    ..
                } => {
                    self.deliver(request_seq, outcome);
                    // Someone may be looking for this answer without waiting for it.
                    wake();
                }
                Message::Event { event, body, .. } => {
                    let _ = self.incoming.send(Incoming::Event { event, body });
                    wake();
                }
                Message::Request {
                    seq,
                    command,
                    arguments,
                } => {
                    let _ = self.incoming.send(Incoming::Request {
                        seq,
                        command,
                        arguments,
                    });
                    wake();
                }
            }
        };
        self.close(&reason);
        let _ = self.incoming.send(Incoming::Closed(reason));
        wake();
    }

    fn deliver(&self, request_seq: i64, outcome: Reply) {
        // An answer to a request nobody waits for any more (it timed out) is dropped.
        if let Some(sender) = lock(&self.pending).waiting.remove(&request_seq) {
            let _ = sender.send(Ok(outcome));
        }
    }

    /// Fails every request still waiting, and every later one.
    fn close(&self, reason: &str) {
        let mut pending = lock(&self.pending);
        pending.closed = Some(reason.to_owned());
        for (_, sender) in pending.waiting.drain() {
            let _ = sender.send(Err(DapError::Closed(reason.to_owned())));
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
