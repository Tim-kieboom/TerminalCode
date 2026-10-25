//! The commands plugins run (`process.run`): started with no shell in a folder of
//! the project, their output passed on in chunks as it comes, and killed with
//! everything they started when the plugin cancels them or is lost.
//!
//! This runs a command; whether it may run (the allowlist, the trusted folder) is
//! decided before, in `routes`.

use std::io::{self, BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use crate::plugin_api::{MAX_RUN_CHUNK, MAX_RUN_OUTPUT, RunStream};

/// How long a command that was told to stop gets before it is killed outright.
const GRACE: Duration = Duration::from_secs(3);
/// How long after the command ended what it left in the pipes is waited for: a process
/// it started may hold them open.
const DRAIN: Duration = Duration::from_secs(2);
/// How often a running command is looked at.
const POLL: Duration = Duration::from_millis(20);
/// Output is passed on when this much has come, or when the pipe has nothing more yet.
const FLUSH_AT: usize = 16 * 1024;

/// What to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Spec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

/// Where the output and the end of a command go.
pub(super) trait Sink: Send + Sync + 'static {
    fn output(&self, stream: RunStream, text: String);
    fn done(&self, code: Option<i32>, cancelled: bool, truncated: bool);
}

const RUNNING: u8 = 0;
const CANCELLED: u8 = 1;
const LOST: u8 = 2;

/// How a running command is stopped.
#[derive(Debug, Default)]
pub(super) struct Control {
    stop: AtomicU8,
}

impl Control {
    /// Stops the command; it ends with [`Sink::done`] saying it was cancelled.
    pub(super) fn cancel(&self) {
        let _ = self
            .stop
            .compare_exchange(RUNNING, CANCELLED, Ordering::SeqCst, Ordering::SeqCst);
    }

    /// Stops the command because nobody is listening any more: it ends without a word.
    pub(super) fn lose(&self) {
        self.stop.store(LOST, Ordering::SeqCst);
    }

    fn stopped(&self) -> u8 {
        self.stop.load(Ordering::SeqCst)
    }

    /// How it was stopped, as a number: 0 not, 1 cancelled, 2 lost.
    #[cfg(test)]
    pub(super) fn stopped_for_test(&self) -> u8 {
        self.stopped()
    }
}

/// Starts the command, which `control` can stop from now on. An error is the command
/// not starting (no such program, no such folder); after `Ok` the sink hears of
/// everything.
pub(super) fn start(spec: Spec, sink: Arc<dyn Sink>, control: Arc<Control>) -> io::Result<()> {
    let (started, outcome) = mpsc::channel();
    thread::Builder::new()
        .name("plugin command".into())
        .spawn(move || match spawn(&spec) {
            Ok(child) => {
                let _ = started.send(Ok(()));
                supervise(child, &control, sink);
            }
            Err(error) => {
                let _ = started.send(Err(error));
            }
        })?;
    outcome
        .recv()
        .unwrap_or_else(|_| Err(io::Error::other("the command's thread ended")))
}

fn spawn(spec: &Spec) -> io::Result<Child> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&spec.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // A group of its own, so that a stop reaches what the command started.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    command.spawn()
}

/// What the readers of the two pipes share.
struct Output {
    sent: AtomicUsize,
    truncated: AtomicBool,
    /// Set when the command is over: nothing more is passed on.
    closed: AtomicBool,
}

fn supervise(mut child: Child, control: &Control, sink: Arc<dyn Sink>) {
    let output = Arc::new(Output {
        sent: AtomicUsize::new(0),
        truncated: AtomicBool::new(false),
        closed: AtomicBool::new(false),
    });
    let (ended, ends) = mpsc::channel();
    let mut readers = 0;
    if let Some(pipe) = child.stdout.take() {
        read_in_thread(pipe, RunStream::Stdout, &sink, &output, &ended);
        readers += 1;
    }
    if let Some(pipe) = child.stderr.take() {
        read_in_thread(pipe, RunStream::Stderr, &sink, &output, &ended);
        readers += 1;
    }
    drop(ended);

    let code = wait(&mut child, control);

    // What the command wrote last is still in the pipes.
    let until = Instant::now() + DRAIN;
    for _ in 0..readers {
        let Some(left) = until.checked_duration_since(Instant::now()) else {
            break;
        };
        if ends.recv_timeout(left).is_err() {
            break;
        }
    }
    output.closed.store(true, Ordering::SeqCst);

    let stop = control.stopped();
    if stop != LOST {
        sink.done(
            code,
            stop == CANCELLED,
            output.truncated.load(Ordering::SeqCst),
        );
    }
}

/// Waits for the command to end, or stops it when told to; its exit code.
fn wait(child: &mut Child, control: &Control) -> Option<i32> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.code(),
            Ok(None) => {}
            Err(_) => return None,
        }
        if control.stopped() != RUNNING {
            return stop(child);
        }
        thread::sleep(POLL);
    }
}

/// Asks the command to end (it may clean up: `cargo nextest` stops its tests), and kills it
/// and everything it started when it does not.
fn stop(child: &mut Child) -> Option<i32> {
    signal_group(child, Signal::Terminate);
    let until = Instant::now() + GRACE;
    while Instant::now() < until {
        if has_exited(child) {
            // What it started may still be there. The child is not reaped yet (see
            // `has_exited`), so its group id is still ours to signal.
            signal_group(child, Signal::Kill);
            return child.wait().ok().and_then(|status| status.code());
        }
        thread::sleep(POLL);
    }
    signal_group(child, Signal::Kill);
    let _ = child.kill();
    child.wait().ok().and_then(|status| status.code())
}

/// Whether the child has ended, without collecting its exit status: until it is waited
/// for, its id (which is also its group's) cannot be given to another process.
#[cfg(unix)]
fn has_exited(child: &Child) -> bool {
    // SAFETY: `siginfo_t` is plain data that all-zero bytes are valid for; `waitid` only
    // writes into it. `WNOWAIT` leaves the child waitable, so its id is still its own.
    unsafe {
        let mut info: libc::siginfo_t = std::mem::zeroed();
        let status = libc::waitid(
            libc::P_PID,
            child.id(),
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        );
        status == 0 && info.si_pid() != 0
    }
}

/// Elsewhere `try_wait` is the only way to look; `Child` keeps the status it collected,
/// and there is no group to signal.
#[cfg(not(unix))]
fn has_exited(child: &mut Child) -> bool {
    matches!(child.try_wait(), Ok(Some(_)))
}

enum Signal {
    Terminate,
    Kill,
}

#[cfg(unix)]
fn signal_group(child: &mut Child, signal: Signal) {
    let Ok(pid) = i32::try_from(child.id()) else {
        return;
    };
    let signal = match signal {
        Signal::Terminate => libc::SIGTERM,
        Signal::Kill => libc::SIGKILL,
    };
    // SAFETY: a signal to the group the child leads (see `spawn`); it sends and touches no
    // memory of ours. Callers signal only a child that has not been waited for (it is either
    // running or ended but not yet collected, see `has_exited`), so its id is still its own.
    unsafe {
        libc::killpg(pid, signal);
    }
}

#[cfg(not(unix))]
fn signal_group(child: &mut Child, signal: Signal) {
    // Only the command itself is reached here.
    if matches!(signal, Signal::Kill) {
        let _ = child.kill();
    }
}

fn read_in_thread(
    pipe: impl Read + Send + 'static,
    stream: RunStream,
    sink: &Arc<dyn Sink>,
    output: &Arc<Output>,
    ended: &mpsc::Sender<()>,
) {
    let (sink, output, ended) = (Arc::clone(sink), Arc::clone(output), ended.clone());
    let spawned = thread::Builder::new()
        .name("plugin command output".into())
        .spawn(move || {
            read(pipe, stream, sink.as_ref(), &output);
            let _ = ended.send(());
        });
    // Without a thread to read it the output is lost, and the command goes on.
    drop(spawned);
}

/// Reads the pipe to its end, passing the text on in chunks: lines are gathered while
/// the pipe has more ready, so a burst of lines is one message.
fn read(pipe: impl Read, stream: RunStream, sink: &dyn Sink, output: &Output) {
    let mut pipe = BufReader::new(pipe);
    let mut pending = Vec::new();
    loop {
        let before = pending.len();
        let room = (MAX_RUN_CHUNK - before) as u64;
        let read = pipe.by_ref().take(room).read_until(b'\n', &mut pending);
        let ended = !matches!(read, Ok(n) if n > 0);
        let flush = ended
            || pending.len() >= FLUSH_AT
            || pending.len() >= MAX_RUN_CHUNK
            || pipe.buffer().is_empty();
        if flush && !pending.is_empty() {
            pass_on(&pending, stream, sink, output);
            pending.clear();
        }
        if ended {
            return;
        }
    }
}

fn pass_on(bytes: &[u8], stream: RunStream, sink: &dyn Sink, output: &Output) {
    if output.closed.load(Ordering::SeqCst) {
        return;
    }
    let before = output.sent.fetch_add(bytes.len(), Ordering::SeqCst);
    if before + bytes.len() > MAX_RUN_OUTPUT {
        output.truncated.store(true, Ordering::SeqCst);
        return;
    }
    sink.output(stream, String::from_utf8_lossy(bytes).into_owned());
}
