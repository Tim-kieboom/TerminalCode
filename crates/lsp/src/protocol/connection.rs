//! One session with a server over a pair of byte streams: requests that wait
//! for their answers, notifications both ways, and a thread that reads.

use std::collections::HashMap;
use std::io::{self, BufReader, Read, Write};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde_json::Value;

use crate::LspError;
use crate::framing::{read_message, write_message};
use crate::message::{Id, Message, RpcError};
use crate::replies;

/// What the server said that is for the app, not for a request in progress.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Notification {
        method: String,
        params: Value,
    },
    /// The connection is over, and why. Always the last thing sent.
    Closed(String),
}

type Reply = Result<Value, RpcError>;
/// Shared by the app and the reading thread, which answers the server's
/// requests. `None` once the session is closed from our side.
type Writer = Arc<Mutex<Option<Box<dyn Write + Send>>>>;

/// The requests that have not been answered yet, and whether they ever will be.
#[derive(Default)]
struct Pending {
    waiting: HashMap<Id, Sender<Result<Reply, LspError>>>,
    /// Set when the connection ended: why.
    closed: Option<String>,
}

type SharedPending = Arc<Mutex<Pending>>;

/// A session with one server. Dropping it closes what we write to, which tells
/// a server on the other end of a pipe that nothing more will come; the
/// reading thread ends when the server closes its side.
pub struct Connection {
    writer: Writer,
    pending: SharedPending,
    next_id: AtomicI64,
}

/// A request that was sent and has not been answered yet.
pub struct PendingReply {
    method: String,
    id: Id,
    answer: Receiver<Result<Reply, LspError>>,
    pending: SharedPending,
}

impl Connection {
    /// Starts reading `reader` on a thread of its own. What the server sends
    /// that is for the app arrives on the returned channel, and `wake` is
    /// called after each such message, so a loop that waits on something else
    /// can look.
    pub fn start(
        reader: impl Read + Send + 'static,
        writer: impl Write + Send + 'static,
        wake: impl Fn() + Send + 'static,
    ) -> (Self, Receiver<Incoming>) {
        let writer: Writer = Arc::new(Mutex::new(Some(Box::new(writer))));
        let pending = SharedPending::default();
        let (incoming, received) = mpsc::channel();

        let reading = Reading {
            writer: Arc::clone(&writer),
            pending: Arc::clone(&pending),
            incoming,
        };
        std::thread::spawn(move || reading.run(reader, &wake));

        let connection = Self {
            writer,
            pending,
            next_id: AtomicI64::new(1),
        };
        (connection, received)
    }

    /// Sends a request. Wait for the answer with [`PendingReply::wait`].
    pub fn request(&self, method: &str, params: Value) -> Result<PendingReply, LspError> {
        let id = Id::Number(self.next_id.fetch_add(1, Ordering::Relaxed));
        let (sender, answer) = mpsc::channel();
        // Registered before it is sent: the answer can be quicker than this thread.
        {
            let mut pending = lock(&self.pending);
            if let Some(reason) = &pending.closed {
                return Err(LspError::Closed(reason.clone()));
            }
            pending.waiting.insert(id.clone(), sender);
        }

        let message = Message::Request {
            id: id.clone(),
            method: method.to_owned(),
            params,
        };
        if let Err(error) = self.send(&message) {
            lock(&self.pending).waiting.remove(&id);
            return Err(error);
        }
        Ok(PendingReply {
            method: method.to_owned(),
            id,
            answer,
            pending: Arc::clone(&self.pending),
        })
    }

    /// Sends a notification, which gets no answer.
    pub fn notify(&self, method: &str, params: Value) -> Result<(), LspError> {
        if let Some(reason) = &lock(&self.pending).closed {
            return Err(LspError::Closed(reason.clone()));
        }
        self.send(&Message::Notification {
            method: method.to_owned(),
            params,
        })
    }

    /// A write that fails means the session is over (a server that exits at
    /// once makes the first write fail before the reading thread has noticed the
    /// end), so from then on every request fails at once.
    fn send(&self, message: &Message) -> Result<(), LspError> {
        write_to(&self.writer, message).map_err(|error| {
            let reason = format!("could not send to the language server: {error}");
            let mut pending = lock(&self.pending);
            let reason = pending.closed.get_or_insert(reason).clone();
            LspError::Closed(reason)
        })
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        lock(&self.writer).take();
    }
}

/// Writes `message`, unless the session was closed from our side.
fn write_to(writer: &Writer, message: &Message) -> io::Result<()> {
    match lock(writer).as_mut() {
        Some(writer) => write_message(writer, &message.to_value()),
        None => Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "the session was closed",
        )),
    }
}

impl PendingReply {
    /// The server's answer, waiting at most `timeout` for it. An answer that
    /// comes after that is dropped.
    pub fn wait(self, timeout: Duration) -> Result<Value, LspError> {
        let outcome = match self.answer.recv_timeout(timeout) {
            Ok(outcome) => outcome,
            Err(RecvTimeoutError::Timeout) => {
                return Err(LspError::Timeout {
                    method: self.method.clone(),
                    seconds: timeout.as_secs(),
                });
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(LspError::Closed("the connection ended".to_owned()));
            }
        };
        self.finish(outcome)
    }

    /// The answer if it has come, without waiting: `None` while it has not.
    pub fn try_take(&mut self) -> Option<Result<Value, LspError>> {
        let outcome = match self.answer.try_recv() {
            Ok(outcome) => outcome,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => {
                return Some(Err(LspError::Closed("the connection ended".to_owned())));
            }
        };
        Some(self.finish(outcome))
    }

    fn finish(&self, outcome: Result<Reply, LspError>) -> Result<Value, LspError> {
        match outcome? {
            Ok(result) => Ok(result),
            Err(error) => Err(LspError::Rejected {
                method: self.method.clone(),
                error,
            }),
        }
    }
}

/// A request nobody waits for any more (it timed out, or its answer is no
/// longer wanted) must not stay in the table of requests that wait.
impl Drop for PendingReply {
    fn drop(&mut self) {
        lock(&self.pending).waiting.remove(&self.id);
    }
}

/// What the reading thread holds.
struct Reading {
    writer: Writer,
    pending: SharedPending,
    incoming: Sender<Incoming>,
}

impl Reading {
    fn run(self, reader: impl Read, wake: &dyn Fn()) {
        let mut reader = BufReader::new(reader);
        let reason = loop {
            let value = match read_message(&mut reader) {
                Ok(Some(value)) => value,
                Ok(None) => break "the server ended the connection".to_owned(),
                Err(error) => break error.to_string(),
            };
            // A message that is not one is skipped: one odd message is not
            // a reason to lose the session.
            let Ok(message) = Message::from_value(value) else {
                continue;
            };
            match message {
                Message::Response { id, outcome } => {
                    self.deliver(&id, outcome);
                    // Someone may be looking for this answer without waiting for it.
                    wake();
                }
                Message::Notification { method, params } => {
                    let _ = self
                        .incoming
                        .send(Incoming::Notification { method, params });
                    wake();
                }
                Message::Request { id, method, params } => {
                    let outcome = replies::answer(&method, &params);
                    let answer = Message::Response { id, outcome };
                    if let Err(error) = write_to(&self.writer, &answer) {
                        break error.to_string();
                    }
                }
            }
        };

        self.close(&reason);
        let _ = self.incoming.send(Incoming::Closed(reason));
        wake();
    }

    fn deliver(&self, id: &Id, outcome: Reply) {
        // An answer to a request nobody waits for any more (it timed out) is dropped.
        if let Some(sender) = lock(&self.pending).waiting.remove(id) {
            let _ = sender.send(Ok(outcome));
        }
    }

    /// Fails every request still waiting, and every later one.
    fn close(&self, reason: &str) {
        let mut pending = lock(&self.pending);
        pending.closed = Some(reason.to_owned());
        for (_, sender) in pending.waiting.drain() {
            let _ = sender.send(Err(LspError::Closed(reason.to_owned())));
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
