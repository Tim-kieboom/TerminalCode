//! Running `git`. A call that may be slow runs on a thread of its own and is
//! killed when it takes too long.

use std::ffi::OsString;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::{GitError, Status, parse_status};

/// How long a call may take before it is given up on.
const TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(10);

/// The way to run `git`: the user's own program, so their config, hooks,
/// credential helpers and ssh agent apply.
#[derive(Debug, Clone)]
pub(crate) struct Git {
    program: OsString,
    timeout: Duration,
}

impl Default for Git {
    fn default() -> Self {
        Self {
            program: OsString::from("git"),
            timeout: TIMEOUT,
        }
    }
}

impl Git {
    /// Runs `program` in place of `git`, and gives up after `timeout`, for tests.
    #[cfg(test)]
    pub(crate) fn with_program(program: impl Into<OsString>, timeout: Duration) -> Self {
        Self {
            program: program.into(),
            timeout,
        }
    }

    /// The status of the repository `project` is in; `root` is its top folder
    /// when that is known already, which saves a call.
    pub(crate) fn refresh(&self, project: &Path, root: Option<&Path>) -> Result<Status, GitError> {
        match root {
            Some(root) => self.status(root),
            None => self.status(&self.repository_root(project)?),
        }
    }

    /// The top folder of the repository `dir` is in.
    pub(crate) fn repository_root(&self, dir: &Path) -> Result<PathBuf, GitError> {
        let output = self.read(dir, &["rev-parse", "--show-toplevel"])?;
        let text = String::from_utf8_lossy(&output);
        match text.trim_end_matches(['\n', '\r']) {
            "" => Err(GitError::Unreadable("no folder for the repository".into())),
            root => Ok(PathBuf::from(root)),
        }
    }

    /// The branch and the changes of the repository whose top folder is `root`.
    pub(crate) fn status(&self, root: &Path) -> Result<Status, GitError> {
        let output = self.read(
            root,
            &[
                "status",
                "--porcelain=v2",
                "--branch",
                "-z",
                "--untracked-files=normal",
            ],
        )?;
        let (branch, entries) = parse_status(&output)?;
        Ok(Status {
            root: root.to_path_buf(),
            branch,
            entries,
        })
    }

    /// Runs a call that only reads: it takes no locks (`--no-optional-locks`
    /// and `GIT_OPTIONAL_LOCKS=0`), because a refresh in the background must not
    /// make a `git commit` the user types in the terminal fail on `index.lock`,
    /// and it never asks for a password.
    fn read(&self, dir: &Path, args: &[&str]) -> Result<Vec<u8>, GitError> {
        let mut command = Command::new(&self.program);
        command
            .arg("--no-optional-locks")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let name = args.first().copied().unwrap_or_default();
        let outcome = run_with_timeout(command, self.timeout).map_err(|error| match error {
            RunError::Spawn(error) if error.kind() == ErrorKind::NotFound => GitError::NotInstalled,
            RunError::Spawn(error) => failed(name, error.to_string()),
            RunError::TimedOut => GitError::TimedOut(self.timeout),
        })?;

        if outcome.success {
            return Ok(outcome.stdout);
        }
        let detail = first_line(&outcome.stderr);
        // 128 is what `git` ends with for "not a repository" and for a folder
        // it will not look at (another owner): both mean there is no git here.
        match (name, outcome.code) {
            ("rev-parse", Some(128)) => Err(GitError::NoRepository(detail)),
            _ => Err(failed(name, detail)),
        }
    }
}

fn failed(command: &str, detail: String) -> GitError {
    GitError::Failed {
        command: command.to_owned(),
        detail,
    }
}

fn first_line(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    text.lines().next().unwrap_or("").trim().to_owned()
}

struct Outcome {
    success: bool,
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

enum RunError {
    Spawn(std::io::Error),
    TimedOut,
}

/// Runs `command` and collects what it printed. The pipes are read on threads of
/// their own: a child that prints more than a pipe holds would wait for them
/// and never end. On a timeout the child is killed and the readers are left to
/// end on their own (a grandchild may still hold the pipes open).
fn run_with_timeout(mut command: Command, timeout: Duration) -> Result<Outcome, RunError> {
    let mut child = command.spawn().map_err(RunError::Spawn)?;
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::TimedOut);
            }
            Err(error) => return Err(RunError::Spawn(error)),
        }
    };
    Ok(Outcome {
        success: status.success(),
        code: status.code(),
        stdout: stdout.map(collect).unwrap_or_default(),
        stderr: stderr.map(collect).unwrap_or_default(),
    })
}

fn drain(mut pipe: impl Read + Send + 'static) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        bytes
    })
}

fn collect(reader: thread::JoinHandle<Vec<u8>>) -> Vec<u8> {
    reader.join().unwrap_or_default()
}
