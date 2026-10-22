//! The runner against real commands, so they are for the platforms that have `sh`.

#![cfg(unix)]

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{Control, Sink, Spec, start};
use crate::plugin_api::{MAX_RUN_CHUNK, MAX_RUN_OUTPUT, RunStream};

#[derive(Debug, PartialEq, Eq)]
enum Heard {
    Output(RunStream, String),
    Done {
        code: Option<i32>,
        cancelled: bool,
        truncated: bool,
    },
}

struct Listener(Mutex<Sender<Heard>>);

impl Sink for Listener {
    fn output(&self, stream: RunStream, text: String) {
        let _ = self.0.lock().unwrap().send(Heard::Output(stream, text));
    }

    fn done(&self, code: Option<i32>, cancelled: bool, truncated: bool) {
        let done = Heard::Done {
            code,
            cancelled,
            truncated,
        };
        let _ = self.0.lock().unwrap().send(done);
    }
}

fn sh(script: &str) -> Spec {
    Spec {
        program: "sh".into(),
        args: vec!["-c".into(), script.into()],
        cwd: std::env::current_dir().unwrap(),
    }
}

fn run(spec: Spec) -> (Arc<Control>, Receiver<Heard>) {
    let (sender, heard) = mpsc::channel();
    let control = Arc::new(Control::default());
    start(
        spec,
        Arc::new(Listener(Mutex::new(sender))),
        Arc::clone(&control),
    )
    .unwrap();
    (control, heard)
}

/// Everything until the end: the text of each stream, and how it ended.
fn finish(heard: &Receiver<Heard>) -> (String, String, Heard) {
    let (mut out, mut err) = (String::new(), String::new());
    loop {
        match heard
            .recv_timeout(Duration::from_secs(30))
            .expect("it ended")
        {
            Heard::Output(RunStream::Stdout, text) => out += &text,
            Heard::Output(_, text) => err += &text,
            done @ Heard::Done { .. } => return (out, err, done),
        }
    }
}

fn ended(code: Option<i32>) -> Heard {
    Heard::Done {
        code,
        cancelled: false,
        truncated: false,
    }
}

#[test]
fn the_output_of_each_stream_comes_in_order_and_then_the_exit_code() {
    let (_, heard) = run(sh("printf 'a\\nb\\n'; printf 'oops\\n' >&2; exit 3"));

    let (out, err, done) = finish(&heard);

    assert_eq!((out.as_str(), err.as_str()), ("a\nb\n", "oops\n"));
    assert_eq!(done, ended(Some(3)));
}

#[test]
fn nothing_comes_after_the_end() {
    let (_, heard) = run(sh("echo hi"));
    finish(&heard);

    assert!(heard.recv_timeout(Duration::from_millis(300)).is_err());
}

#[test]
fn a_program_that_is_not_there_is_an_error_not_an_end() {
    let (sender, _heard) = mpsc::channel();
    let spec = Spec {
        program: "trcode-no-such-program".into(),
        args: Vec::new(),
        cwd: std::env::current_dir().unwrap(),
    };

    let control = Arc::new(Control::default());
    let error = start(spec, Arc::new(Listener(Mutex::new(sender))), control).unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn a_folder_that_is_not_there_is_an_error() {
    let (sender, _heard) = mpsc::channel();
    let mut spec = sh("true");
    spec.cwd = "/trcode/no/such/folder".into();

    let control = Arc::new(Control::default());
    assert!(start(spec, Arc::new(Listener(Mutex::new(sender))), control).is_err());
}

#[test]
fn the_command_runs_in_the_folder_it_was_given() {
    let folder = tempfile::tempdir().unwrap();
    let mut spec = sh("pwd -P");
    spec.cwd = folder.path().to_path_buf();
    let (_, heard) = run(spec);

    let (out, _, _) = finish(&heard);

    let expected = folder.path().canonicalize().unwrap();
    assert_eq!(out.trim_end(), expected.to_str().unwrap());
}

#[test]
fn arguments_are_never_read_by_a_shell() {
    let spec = Spec {
        program: "echo".into(),
        args: vec!["a; echo injected $HOME `id`".into()],
        cwd: std::env::current_dir().unwrap(),
    };
    let (_, heard) = run(spec);

    let (out, _, _) = finish(&heard);

    assert_eq!(out, "a; echo injected $HOME `id`\n");
}

#[test]
fn the_standard_input_of_a_command_is_empty() {
    let (_, heard) = run(sh("cat; echo after"));

    let (out, _, done) = finish(&heard);

    assert_eq!(out, "after\n");
    assert_eq!(done, ended(Some(0)));
}

#[test]
fn a_cancel_stops_the_command_and_says_so() {
    let (control, heard) = run(sh("echo started; sleep 30"));
    assert!(matches!(
        heard.recv_timeout(Duration::from_secs(10)),
        Ok(Heard::Output(..))
    ));
    let began = Instant::now();

    control.cancel();

    let (_, _, done) = finish(&heard);
    assert!(
        matches!(
            done,
            Heard::Done {
                cancelled: true,
                ..
            }
        ),
        "{done:?}"
    );
    assert!(began.elapsed() < Duration::from_secs(10));
}

/// Whether the process is still there (a process that ended but was not waited for is
/// there too, and ours is not: it is a child of `init` once its parent is gone).
fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only asks whether the process exists.
    unsafe { libc::kill(pid, 0) == 0 }
}

#[test]
fn a_cancel_reaches_what_the_command_started() {
    let (control, heard) = run(sh("sleep 60 & echo $!; wait"));
    let Ok(Heard::Output(_, text)) = heard.recv_timeout(Duration::from_secs(10)) else {
        panic!("it said nothing");
    };
    let pid: i32 = text.trim().parse().unwrap();
    assert!(alive(pid));

    control.cancel();
    finish(&heard);

    let until = Instant::now() + Duration::from_secs(5);
    while alive(pid) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !alive(pid),
        "the process the command started is still there"
    );
}

#[test]
fn a_command_that_is_lost_is_killed_and_says_nothing() {
    let (control, heard) = run(sh("echo started; sleep 60 & echo $!; wait"));
    let mut said = String::new();
    while !said.contains('\n') || said.lines().count() < 2 {
        let Ok(Heard::Output(_, text)) = heard.recv_timeout(Duration::from_secs(10)) else {
            panic!("it said too little: {said:?}");
        };
        said += &text;
    }
    let pid: i32 = said.lines().nth(1).unwrap().trim().parse().unwrap();

    control.lose();

    let until = Instant::now() + Duration::from_secs(10);
    while alive(pid) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!alive(pid));
    assert!(heard.recv_timeout(Duration::from_millis(500)).is_err());
}

#[test]
fn a_process_left_holding_the_pipes_does_not_hold_up_the_end() {
    let (_, heard) = run(sh("sleep 3 & echo main"));
    let began = Instant::now();

    let (out, _, done) = finish(&heard);

    assert_eq!(out, "main\n");
    assert_eq!(done, ended(Some(0)));
    assert!(began.elapsed() < Duration::from_millis(2900));
}

#[test]
fn a_burst_of_lines_is_few_messages_and_all_of_the_text() {
    let (_, heard) = run(sh("seq 1 20000"));
    let (mut text, mut messages) = (String::new(), 0);

    while let Heard::Output(_, chunk) = heard.recv_timeout(Duration::from_secs(30)).unwrap() {
        assert!(chunk.len() <= MAX_RUN_CHUNK);
        text += &chunk;
        messages += 1;
    }

    let expected: String = (1..=20000).map(|n| format!("{n}\n")).collect();
    assert_eq!(text, expected);
    assert!(messages < 2000, "{messages} messages for 20000 lines");
}

#[test]
fn a_line_longer_than_a_chunk_is_cut_into_chunks() {
    let (_, heard) = run(sh("head -c 300000 /dev/zero | tr '\\0' x"));
    let mut longest = 0;
    let mut total = 0;

    while let Heard::Output(_, chunk) = heard.recv_timeout(Duration::from_secs(30)).unwrap() {
        longest = longest.max(chunk.len());
        total += chunk.len();
    }

    assert_eq!(total, 300_000);
    assert!(longest <= MAX_RUN_CHUNK);
}

#[test]
fn bytes_that_are_not_text_are_replaced() {
    let (_, heard) = run(sh("printf 'a\\377b'"));

    let (out, _, _) = finish(&heard);

    assert_eq!(out, "a\u{fffd}b");
}

#[test]
fn output_over_the_limit_is_dropped_and_the_end_says_truncated() {
    let over = MAX_RUN_OUTPUT + 4 * 1024 * 1024;
    let (_, heard) = run(sh(&format!("head -c {over} /dev/zero | tr '\\0' 'x'")));
    let mut total = 0;

    let done = loop {
        match heard.recv_timeout(Duration::from_secs(60)).unwrap() {
            Heard::Output(_, chunk) => total += chunk.len(),
            done @ Heard::Done { .. } => break done,
        }
    };

    assert!(total <= MAX_RUN_OUTPUT, "{total}");
    assert_eq!(
        done,
        Heard::Done {
            code: Some(0),
            cancelled: false,
            truncated: true
        }
    );
}
