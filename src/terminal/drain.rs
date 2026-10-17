//! Reading what the terminal sent before mouse reporting went off. With all-motion
//! tracking there is a report for every cell the pointer crosses; any that nobody read
//! stays in the tty input and the shell then reads it as typed text. A status request
//! (`CSI 5 n`) is answered (`CSI 0 n`) after everything the terminal sent earlier, so
//! reading until the answer comes leaves nothing behind.

use std::time::{Duration, Instant};

/// The terminal's answer to `CSI 5 n`: it is fine.
const ANSWER: &[u8] = b"\x1b[0n";

/// How long to wait for the answer. A terminal that never answers costs this much once.
#[cfg(unix)]
pub(super) const TIMEOUT: Duration = Duration::from_millis(50);

/// Where the terminal's bytes come from.
pub(super) trait Input {
    /// What arrived within `wait`: `None` when nothing did.
    fn read(&mut self, wait: Duration) -> Option<Vec<u8>>;
}

/// How reading ended.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Drained {
    /// The answer came, so every earlier report was read.
    Answered,
    /// It did not come in time (or the input ended).
    TimedOut,
}

/// Reads and drops input until the answer to the status request has arrived, or
/// `timeout` has passed.
pub(super) fn until_answered(input: &mut impl Input, timeout: Duration) -> Drained {
    let deadline = Instant::now() + timeout;
    // The end of what came so far, in case the answer is cut between two reads.
    let mut seen: Vec<u8> = Vec::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Drained::TimedOut;
        }
        let Some(bytes) = input.read(remaining) else {
            return Drained::TimedOut;
        };
        seen.extend_from_slice(&bytes);
        if seen.windows(ANSWER.len()).any(|window| window == ANSWER) {
            return Drained::Answered;
        }
        let keep = seen.len().saturating_sub(ANSWER.len() - 1);
        seen.drain(..keep);
    }
}

/// Asks the terminal for its status and drops what it sent before answering. Call it
/// after mouse reporting was switched off and while still in raw mode.
#[cfg(unix)]
pub(super) fn flush_pending_reports() {
    use std::io::Write;

    let Some(mut input) = Tty::open() else {
        return;
    };
    let mut out = std::io::stdout();
    if out
        .write_all(b"\x1b[5n")
        .and_then(|()| out.flush())
        .is_err()
    {
        return;
    }
    until_answered(&mut input, TIMEOUT);
}

/// Windows delivers input as console events, not as text a shell would read.
#[cfg(not(unix))]
pub(super) fn flush_pending_reports() {}

/// The controlling terminal, read straight from the tty.
#[cfg(unix)]
struct Tty(std::fs::File);

#[cfg(unix)]
impl Tty {
    fn open() -> Option<Self> {
        std::fs::File::open("/dev/tty").ok().map(Self)
    }
}

#[cfg(unix)]
impl Input for Tty {
    fn read(&mut self, wait: Duration) -> Option<Vec<u8>> {
        use std::io::Read;
        use std::os::fd::AsRawFd;

        let mut fd = libc::pollfd {
            fd: self.0.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let millis = wait.as_millis().clamp(1, i32::MAX as u128) as i32;
        // SAFETY: `fd` is one valid pollfd and the count says so.
        let ready = unsafe { libc::poll(&mut fd, 1, millis) };
        if ready <= 0 {
            return None;
        }
        let mut buffer = [0u8; 256];
        match self.0.read(&mut buffer) {
            Ok(0) | Err(_) => None,
            Ok(count) => Some(buffer[..count].to_vec()),
        }
    }
}
