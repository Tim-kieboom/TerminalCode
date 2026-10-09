use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

use super::queue::BoundedQueue;
use super::{PtyError, Shell};

/// How much a write may hand to the writer thread in one piece.
const CHUNK: usize = 4096;
/// How many pieces wait for the writer thread: 256 KB.
const QUEUED_CHUNKS: usize = 64;
/// How long a write waits for room before it gives up on the rest.
const WAIT_FOR_ROOM: Duration = Duration::from_millis(20);
/// How much of the child's output the reader parses between two looks at the
/// lock; small, so drawing never waits long.
const READ_SLICE: usize = 8 * 1024;
/// Lines of history the screen keeps.
const SCROLLBACK: usize = 10_000;

/// What to start and where.
#[derive(Debug, Clone)]
pub(crate) struct SpawnConfig {
    pub(crate) shell: Shell,
    pub(crate) directory: PathBuf,
    pub(crate) rows: u16,
    pub(crate) columns: u16,
}

/// What a write got through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Written {
    /// Bytes the writer thread was given; the rest were dropped.
    pub(crate) sent: usize,
    pub(crate) complete: bool,
}

/// A running shell. Dropping it stops the shell.
///
/// Output is parsed on the reader thread, which keeps the screen up to date
/// behind a lock the drawing thread takes only to look. Input goes through a
/// queue to a writer thread, because a child that is not reading makes writes
/// to it block, and that must not be the thread that draws.
pub(crate) struct Session {
    shared: Arc<Shared>,
    input: Arc<BoundedQueue<Message>>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    process_id: Option<u32>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("process_id", &self.process_id)
            .finish_non_exhaustive()
    }
}

struct Shared {
    screen: Mutex<vt100::Parser>,
    /// A wake-up has gone out that no draw has answered yet.
    wake_pending: AtomicBool,
    /// The reader has seen the end of the child's output.
    ended: AtomicBool,
    /// The size the child should have, until the writer thread applies it.
    wanted_size: Mutex<Option<PtySize>>,
    /// Everything the child has printed, for tests.
    #[cfg(test)]
    received: std::sync::atomic::AtomicU64,
}

enum Message {
    Input(Vec<u8>),
    /// Look at `Shared::wanted_size`.
    Resize,
}

impl Session {
    /// Starts the shell. `wake` is called, from another thread, when the
    /// screen has changed since it was last drawn (see [`Session::take_dirty`]);
    /// it is called once, not once per change, until a draw has taken the
    /// change.
    pub(crate) fn spawn(
        config: &SpawnConfig,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, PtyError> {
        let size = size_of(config.rows, config.columns);
        let pair = native_pty_system()
            .openpty(size)
            .map_err(|error| PtyError::Open(error.to_string()))?;

        let mut command = CommandBuilder::new(&config.shell.program);
        command.args(&config.shell.args);
        command.cwd(&config.directory);
        command.env("TERM", "xterm-256color");
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| PtyError::Spawn {
                program: config.shell.program.clone(),
                message: error.to_string(),
            })?;
        // The child has its end; ours must go so that the reader sees the end
        // of the output when the child exits.
        drop(pair.slave);
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| PtyError::Open(error.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| PtyError::Open(error.to_string()))?;

        let shared = Arc::new(Shared {
            screen: Mutex::new(vt100::Parser::new(config.rows, config.columns, SCROLLBACK)),
            wake_pending: AtomicBool::new(false),
            ended: AtomicBool::new(false),
            wanted_size: Mutex::new(None),
            #[cfg(test)]
            received: std::sync::atomic::AtomicU64::new(0),
        });
        let input = Arc::new(BoundedQueue::new(QUEUED_CHUNKS));
        let process_id = child.process_id();

        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        let (reading, waking) = (Arc::clone(&shared), Arc::clone(&wake));
        thread::spawn(move || read_output(reader, &reading, &*waking));
        let (writing, queue) = (Arc::clone(&shared), Arc::clone(&input));
        thread::spawn(move || write_input(writer, pair.master, &writing, &queue));

        Ok(Self {
            shared,
            input,
            child: Arc::new(Mutex::new(child)),
            process_id,
        })
    }

    /// Sends `bytes` to the shell. Waits at most 20 ms for room: if the shell
    /// is not reading and the queue stays full, the rest is dropped, and the
    /// result says how much went through.
    pub(crate) fn write(&self, bytes: &[u8]) -> Written {
        let mut sent = 0;
        for chunk in bytes.chunks(CHUNK) {
            let queued = self
                .input
                .push_within(Message::Input(chunk.to_vec()), WAIT_FOR_ROOM);
            if queued.is_err() {
                return Written {
                    sent,
                    complete: false,
                };
            }
            sent += chunk.len();
        }
        Written {
            sent,
            complete: true,
        }
    }

    /// Gives the screen and the shell a new size. The screen changes at once;
    /// the shell hears about it from the writer thread, in order with the input
    /// before it.
    pub(crate) fn resize(&self, rows: u16, columns: u16) {
        self.lock_screen().screen_mut().set_size(rows, columns);
        *self
            .shared
            .wanted_size
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(size_of(rows, columns));
        // If the queue is full the writer applies the size after its next item.
        let _ = self.input.push_within(Message::Resize, Duration::ZERO);
    }

    /// Whether the screen changed since the last call, and says the next
    /// change should wake the app again. Call it before drawing.
    pub(crate) fn take_dirty(&self) -> bool {
        self.shared.wake_pending.swap(false, Ordering::SeqCst)
    }

    /// Looks at the screen. Holds the lock for the duration of `look`, so keep
    /// it short.
    pub(crate) fn with_screen<R>(&self, look: impl FnOnce(&vt100::Screen) -> R) -> R {
        look(self.lock_screen().screen())
    }

    /// The exit code, once the shell has ended.
    pub(crate) fn exit_code(&self) -> Option<u32> {
        let mut child = self
            .child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        child
            .try_wait()
            .ok()
            .flatten()
            .map(|status| status.exit_code())
    }

    /// How many bytes the child has printed so far.
    #[cfg(test)]
    pub(crate) fn bytes_received(&self) -> u64 {
        self.shared.received.load(Ordering::Relaxed)
    }

    /// Whether the shell has closed its end of the terminal, which is what
    /// happens when it exits.
    pub(crate) fn output_ended(&self) -> bool {
        self.shared.ended.load(Ordering::SeqCst)
    }

    /// The exit code of a shell that is over. The end of its output comes a
    /// moment before it can be reaped, so this waits a little for that; `None`
    /// if it is still running.
    pub(crate) fn finished(&self) -> Option<u32> {
        if let Some(code) = self.exit_code() {
            return Some(code);
        }
        if !self.output_ended() {
            return None;
        }
        for _ in 0..40 {
            thread::sleep(Duration::from_millis(5));
            if let Some(code) = self.exit_code() {
                return Some(code);
            }
        }
        None
    }

    pub(crate) fn process_id(&self) -> Option<u32> {
        self.process_id
    }

    fn lock_screen(&self) -> MutexGuard<'_, vt100::Parser> {
        self.shared
            .screen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.input.close();
        let mut child = self
            .child
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Already gone is fine.
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn size_of(rows: u16, columns: u16) -> PtySize {
    PtySize {
        rows,
        cols: columns,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Feeds the child's output to the screen until it ends.
fn read_output(mut reader: Box<dyn Read + Send>, shared: &Shared, wake: &(dyn Fn() + Send + Sync)) {
    let mut buffer = [0u8; READ_SLICE];
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        shared
            .screen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .process(&buffer[..read]);
        #[cfg(test)]
        shared.received.fetch_add(read as u64, Ordering::Relaxed);
        wake_once(shared, wake);
    }
    shared.ended.store(true, Ordering::SeqCst);
    // The shell ended; the app wants to know.
    wake_once(shared, wake);
}

/// Wakes the app unless a wake-up is still waiting for its draw.
fn wake_once(shared: &Shared, wake: &(dyn Fn() + Send + Sync)) {
    if !shared.wake_pending.swap(true, Ordering::SeqCst) {
        wake();
    }
}

/// Writes queued input to the child, and passes size changes on. It stops
/// when the queue closes or the child stops listening.
fn write_input(
    mut writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    shared: &Shared,
    queue: &BoundedQueue<Message>,
) {
    while let Some(message) = queue.pop() {
        let wanted = shared
            .wanted_size
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(size) = wanted {
            let _ = master.resize(size);
        }
        if let Message::Input(bytes) = message
            && (writer.write_all(&bytes).is_err() || writer.flush().is_err())
        {
            queue.close();
            return;
        }
    }
}
