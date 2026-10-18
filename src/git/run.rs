//! Running `git`. A call that may be slow runs on a thread of its own and is
//! killed when it takes too long.

use std::ffi::OsString;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::{GitError, Status, hash_lines, parse_status};

/// How long a call may take before it is given up on.
const TIMEOUT: Duration = Duration::from_secs(5);
/// The largest file whose lines are compared with the index.
const MAX_BASE_BYTES: usize = 8 * 1024 * 1024;
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

    /// The hash of each line of the file `relative` (a path from the root of the
    /// repository) as the index has it, to compare the buffer with:
    /// - `Some(hashes)`: the lines of the index text;
    /// - `Some(vec![])`: git does not have the file yet, all its lines are new;
    /// - `None`: nothing to compare with: a file that is ignored, one that is not
    ///   text, or one that is too big.
    pub(crate) fn index_lines(
        &self,
        root: &Path,
        relative: &Path,
    ) -> Result<Option<Vec<u64>>, GitError> {
        let path = slashed(relative);
        let shown = self.run(root, &["show", &format!(":0:{path}")])?;
        if shown.success {
            return Ok(String::from_utf8(shown.stdout)
                .ok()
                .filter(|text| text.len() <= MAX_BASE_BYTES)
                .map(|text| hash_lines(&text)));
        }
        // 128 is what `show` ends with for a path that is not in the index.
        if shown.code != Some(128) {
            return Err(failed("show", first_line(&shown.stderr)));
        }
        let ignored = self.run(root, &["check-ignore", "-q", "--", &path])?;
        match ignored.code {
            Some(0) => Ok(None),
            Some(1) => Ok(Some(Vec::new())),
            // A path in a submodule, or one git refuses for another reason.
            _ => Ok(None),
        }
    }

    /// Runs a call that only reads and is expected to succeed.
    fn read(&self, dir: &Path, args: &[&str]) -> Result<Vec<u8>, GitError> {
        let name = args.first().copied().unwrap_or_default();
        let outcome = self.run(dir, args)?;
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

    /// Runs a call that only reads: it takes no locks (`--no-optional-locks`
    /// and `GIT_OPTIONAL_LOCKS=0`), because a refresh in the background must not
    /// make a `git commit` the user types in the terminal fail on `index.lock`,
    /// and it never asks for a password. Whether it succeeded is for the caller
    /// to look at; an error is only that it could not run or took too long.
    fn run(&self, dir: &Path, args: &[&str]) -> Result<Outcome, GitError> {
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
        run_with_timeout(command, self.timeout).map_err(|error| match error {
            RunError::Spawn(error) if error.kind() == ErrorKind::NotFound => GitError::NotInstalled,
            RunError::Spawn(error) => failed(name, error.to_string()),
            RunError::TimedOut => GitError::TimedOut(self.timeout),
        })
    }
}

/// `path` with `/` between its parts, which is how git names a path on every platform.
fn slashed(path: &Path) -> String {
    let parts: Vec<_> = path
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect();
    parts.join("/")
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
