//! The real launcher, with small shell scripts standing in for servers. Unix
//! only: they need `sh`.
#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::lsp::{Incoming, LaunchSpec, Launcher, LspError, ProcessLauncher, StartError};

use super::support::PATIENCE;

/// Answers `initialize` (it waits for the request's first header line, so the
/// request is already waiting for its answer) and then runs `after`.
fn server(before: &str, after: &str) -> String {
    format!(
        r#"{before}
IFS= read -r header
body='{{"jsonrpc":"2.0","id":1,"result":{{"capabilities":{{}},"serverInfo":{{"name":"script-server"}}}}}}'
printf 'Content-Length: %d\r\n\r\n%s' "${{#body}}" "$body"
{after}"#
    )
}

fn spec(root: &Path, script: &str) -> LaunchSpec {
    LaunchSpec {
        name: "script".to_owned(),
        command: "sh".to_owned(),
        args: vec!["-c".to_owned(), script.to_owned()],
        root: root.to_path_buf(),
        initialization_options: Value::Null,
    }
}

fn launch(spec: &LaunchSpec) -> Result<crate::lsp::Session, StartError> {
    ProcessLauncher.launch(spec, Arc::new(|| {}))
}

#[test]
fn a_program_that_does_not_exist_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let mut spec = spec(dir.path(), "");
    spec.command = "/nonexistent/language-server".to_owned();

    let Err(error) = launch(&spec) else {
        panic!("a missing program started");
    };

    assert!(matches!(error, StartError::Spawn(_)), "{error:?}");
    assert!(
        error.reason().contains("was not found"),
        "{}",
        error.reason()
    );
    assert!(
        error.reason().contains("/nonexistent/language-server"),
        "{}",
        error.reason()
    );
}

#[test]
fn a_program_that_prints_why_and_exits_at_once_has_its_first_line_reported() {
    let dir = tempfile::tempdir().unwrap();
    // What the rustup shim does for a component that is not installed.
    let script = "echo \"error: Unknown binary 'rust-analyzer' in official toolchain 'stable'\" >&2; echo 'second line' >&2; exit 1";

    let Err(error) = launch(&spec(dir.path(), script)) else {
        panic!("a program that exited started");
    };

    let StartError::Handshake { error: why, stderr } = &error else {
        panic!("{error:?}");
    };
    assert!(matches!(why, LspError::Closed(_)), "{why}");
    assert_eq!(
        stderr.as_deref(),
        Some("error: Unknown binary 'rust-analyzer' in official toolchain 'stable'")
    );
    assert!(
        error.reason().contains("Unknown binary"),
        "{}",
        error.reason()
    );
}

#[test]
fn a_program_that_exits_in_silence_is_reported_without_a_line() {
    let dir = tempfile::tempdir().unwrap();

    let Err(StartError::Handshake { stderr, .. }) = launch(&spec(dir.path(), "exit 3")) else {
        panic!("a program that exited started");
    };

    assert_eq!(stderr, None);
}

#[test]
fn a_program_that_speaks_the_protocol_starts_and_names_itself() {
    let dir = tempfile::tempdir().unwrap();

    let session = launch(&spec(dir.path(), &server("", "sleep 30"))).unwrap();

    assert_eq!(session.server.name.as_deref(), Some("script-server"));
}

#[test]
fn the_program_runs_in_the_project_root() {
    let dir = tempfile::tempdir().unwrap();
    let script = server("pwd -P > where.txt", "sleep 30");

    let _session = launch(&spec(dir.path(), &script)).unwrap();

    let canonical = fs::canonicalize(dir.path()).unwrap();
    let said = fs::read_to_string(dir.path().join("where.txt")).unwrap();
    assert_eq!(said.trim(), canonical.to_str().unwrap());
}

#[test]
fn the_initialization_options_reach_the_program_in_the_request() {
    let dir = tempfile::tempdir().unwrap();
    let script = r#"
IFS= read -r header
IFS= read -r blank
length=$(printf %s "$header" | tr -dc 0-9)
head -c "$length" > request.json
body='{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}'
printf 'Content-Length: %d\r\n\r\n%s' "${#body}" "$body"
sleep 30"#;
    let mut spec = spec(dir.path(), script);
    spec.initialization_options = json!({ "cargo": { "features": "all" } });

    let _session = launch(&spec).unwrap();

    let request: Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join("request.json")).unwrap())
            .unwrap();
    assert_eq!(request["method"], json!("initialize"));
    assert_eq!(
        request["params"]["initializationOptions"],
        json!({ "cargo": { "features": "all" } })
    );
    assert_eq!(
        request["params"]["capabilities"]["general"]["positionEncodings"],
        json!(["utf-16"])
    );
}

#[test]
fn a_program_that_floods_its_error_output_does_not_block_the_start() {
    let dir = tempfile::tempdir().unwrap();
    // Far more than a pipe holds, before it answers.
    let script = server(
        "yes 'a long line of complaints that nobody reads' | head -c 2000000 >&2",
        "sleep 30",
    );

    let session = launch(&spec(dir.path(), &script));

    assert!(
        session.is_ok(),
        "{:?}",
        session.err().map(|error| error.reason())
    );
}

#[test]
fn a_program_that_dies_after_starting_ends_the_session() {
    let dir = tempfile::tempdir().unwrap();

    let session = launch(&spec(dir.path(), &server("", "exit 0"))).unwrap();

    let deadline = Instant::now() + PATIENCE;
    loop {
        match session.incoming.recv_timeout(PATIENCE) {
            Ok(Incoming::Closed(_)) => break,
            Ok(_) => {}
            Err(error) => panic!("the end was never reported: {error}"),
        }
        assert!(Instant::now() < deadline);
    }
}

#[test]
fn dropping_the_session_ends_the_program() {
    let dir = tempfile::tempdir().unwrap();
    let script = server("echo $$ > pid.txt", "sleep 60");
    let session = launch(&spec(dir.path(), &script)).unwrap();
    let pid = fs::read_to_string(dir.path().join("pid.txt"))
        .unwrap()
        .trim()
        .to_owned();
    assert!(alive(&pid), "it should be running");

    drop(session);

    let deadline = Instant::now() + PATIENCE;
    while alive(&pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!alive(&pid), "the program was left running");
}

fn alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[test]
fn a_failed_start_leaves_no_program_behind() {
    let dir = tempfile::tempdir().unwrap();
    // Reads the request but never answers: the handshake times out. A shorter
    // wait than the real one would need a hook; the program exiting is enough here.
    let script = "echo $$ > pid.txt; exit 1";
    let Err(_) = launch(&spec(dir.path(), script)) else {
        panic!("started");
    };

    let pid = fs::read_to_string(dir.path().join("pid.txt"))
        .unwrap()
        .trim()
        .to_owned();
    assert!(!alive(&pid));
}

/// The real `rust-analyzer`, on a small crate: it starts, names itself, reports
/// what it is doing and ends when told to. Run by hand, since CI has no
/// `rust-analyzer`: `cargo test real_rust_analyzer -- --ignored --nocapture`.
#[test]
#[ignore = "needs rust-analyzer on the PATH; run by hand"]
fn real_rust_analyzer_starts_reports_progress_and_shuts_down() {
    use crate::lsp::shutdown;
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"small\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    let spec = LaunchSpec {
        name: "rust".to_owned(),
        command: "rust-analyzer".to_owned(),
        args: Vec::new(),
        root: dir.path().to_path_buf(),
        initialization_options: Value::Null,
    };

    let session = match launch(&spec) {
        Ok(session) => session,
        Err(error) => panic!("rust-analyzer did not start: {}", error.reason()),
    };

    eprintln!("server: {:?}", session.server.name);
    assert_eq!(session.server.name.as_deref(), Some("rust-analyzer"));
    // It says what it is busy with while it loads the workspace.
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut saw_progress = false;
    while Instant::now() < deadline && !saw_progress {
        if let Ok(Incoming::Notification { method, params }) =
            session.incoming.recv_timeout(Duration::from_millis(200))
            && method == "$/progress"
        {
            eprintln!("progress: {params}");
            saw_progress = true;
        }
    }
    assert!(saw_progress, "no progress was reported in a minute");
    shutdown(&session.connection, Duration::from_secs(20)).unwrap();
}
