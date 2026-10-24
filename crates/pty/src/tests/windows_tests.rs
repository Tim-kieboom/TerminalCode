//! The terminal on Windows (ConPTY), with `cmd.exe` and with `sh`, which the
//! other terminal tests use. A failure says what the session saw.

use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use crate::{Session, Shell, SpawnConfig};

struct Seen {
    screen: String,
    bytes: u64,
    exit_code: Option<u32>,
    output_ended: bool,
}

/// Runs `shell` in `directory` and says what the session saw by the time
/// `done` holds, or after eight seconds.
fn run(shell: Shell, directory: &Path, done: impl Fn(&Session) -> bool) -> Seen {
    let config = SpawnConfig {
        shell,
        directory: directory.to_path_buf(),
        env: Vec::new(),
        rows: 24,
        columns: 80,
    };
    let session = match Session::spawn(&config, || {}) {
        Ok(session) => session,
        Err(error) => panic!("could not start: {error}"),
    };
    let deadline = Instant::now() + Duration::from_secs(8);
    while !done(&session) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    Seen {
        screen: session.with_screen(|screen| screen.contents()),
        bytes: session.bytes_received(),
        exit_code: session.exit_code(),
        output_ended: session.output_ended(),
    }
}

fn describe(seen: &Seen) -> String {
    format!(
        "screen {:?}, {} bytes received, exit code {:?}, output ended {}",
        seen.screen, seen.bytes, seen.exit_code, seen.output_ended
    )
}

fn plain_directory() -> (tempfile::TempDir, PathBuf) {
    let dir = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(error) => panic!("no temporary directory: {error}"),
    };
    let path = dir.path().to_path_buf();
    (dir, path)
}

fn cmd(arguments: &[&str]) -> Shell {
    Shell {
        program: "cmd.exe".to_owned(),
        args: arguments
            .iter()
            .map(|argument| (*argument).to_owned())
            .collect(),
    }
}

#[test]
fn cmd_prints_to_the_screen() {
    let (_dir, path) = plain_directory();

    let seen = run(cmd(&["/C", "echo hello"]), &path, |session| {
        session.with_screen(|screen| screen.contents().contains("hello"))
    });

    assert!(seen.screen.contains("hello"), "{}", describe(&seen));
}

#[test]
fn cmd_reports_its_exit_code() {
    let (_dir, path) = plain_directory();

    let seen = run(cmd(&["/C", "exit 3"]), &path, |session| {
        session.exit_code().is_some()
    });

    assert_eq!(seen.exit_code, Some(3), "{}", describe(&seen));
}

#[test]
fn the_output_ends_when_cmd_exits() {
    let (_dir, path) = plain_directory();

    let seen = run(cmd(&["/C", "echo bye"]), &path, |session| {
        session.output_ended()
    });

    assert!(seen.output_ended, "{}", describe(&seen));
}

#[test]
fn cmd_still_runs_in_a_directory_with_a_verbatim_prefix() {
    let (_dir, path) = plain_directory();
    let verbatim = match path.canonicalize() {
        Ok(verbatim) => verbatim,
        Err(error) => panic!("cannot canonicalize {}: {error}", path.display()),
    };

    let seen = run(cmd(&["/C", "echo hello"]), &verbatim, |session| {
        session.with_screen(|screen| screen.contents().contains("hello"))
    });

    assert!(
        seen.screen.contains("hello"),
        "{} in {}",
        describe(&seen),
        verbatim.display()
    );
}

#[test]
fn sh_prints_to_the_screen() {
    let (_dir, path) = plain_directory();

    let seen = run(Shell::script("printf hello"), &path, |session| {
        session.with_screen(|screen| screen.contents().contains("hello"))
    });

    assert!(seen.screen.contains("hello"), "{}", describe(&seen));
}

/// The name of the last folder of `path`, which the shell's `cd` prints in
/// whichever spelling it likes of the folders before it.
fn folder_name(path: &Path) -> String {
    match path.file_name() {
        Some(name) => name.to_string_lossy().into_owned(),
        None => panic!("{} has no folder name", path.display()),
    }
}

#[test]
fn cmd_starts_in_the_directory_asked_for() {
    let (_dir, path) = plain_directory();
    let name = folder_name(&path);

    let seen = run(cmd(&["/C", "cd"]), &path, |session| {
        session.with_screen(|screen| screen.contents().contains(&name))
    });

    assert!(seen.screen.contains(&name), "{}", describe(&seen));
}

#[test]
fn a_directory_with_a_verbatim_prefix_is_used_without_it() {
    let (_dir, path) = plain_directory();
    let name = folder_name(&path);
    let verbatim = match path.canonicalize() {
        Ok(verbatim) => verbatim,
        Err(error) => panic!("cannot canonicalize {}: {error}", path.display()),
    };

    let seen = run(cmd(&["/C", "cd"]), &verbatim, |session| {
        session.with_screen(|screen| screen.contents().contains(&name))
    });

    assert!(seen.screen.contains(&name), "{}", describe(&seen));
    assert!(!seen.screen.contains("UNC"), "{}", describe(&seen));
    assert!(!seen.screen.contains(r"\\?\"), "{}", describe(&seen));
}
