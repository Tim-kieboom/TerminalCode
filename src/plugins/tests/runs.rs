//! Plugins running commands, through the runtime and real modules and real commands
//! (so for the platforms that have `sh`): the allowlist, trust, output, cancel, and what
//! happens to a command when its plugin is lost.

#![cfg(unix)]

use std::time::{Duration, Instant};

use super::calls::{act, heard, never_heard, text_of};
use super::{Rig, rig};
use crate::plugin_api::{Capability, RunPattern, ToPlugin};
use crate::plugins::fixtures::{Echo, First};
use crate::plugins::{Notice, Thread};

const RUN_ECHO: &str =
    r#"{"type":"run","request":5,"program":"sh","args":["-c","echo hi; echo oops >&2; exit 4"]}"#;
const RUN_SLEEP: &str =
    r#"{"type":"run","request":5,"program":"sh","args":["-c","echo $$; exec sleep 60"]}"#;
const RUN_LS: &str = r#"{"type":"run","request":5,"program":"ls"}"#;
const RUN_MISSING: &str = r#"{"type":"run","request":5,"program":"trcode-no-such-program"}"#;
const CANCEL: &str = r#"{"type":"run_cancel","request":5}"#;

const GOT_HI: &str = r#"{"type":"run_output","request":5,"stream":"stdout","text":"hi\n"}"#;
const GOT_OOPS: &str = r#"{"type":"run_output","request":5,"stream":"stderr","text":"oops\n"}"#;
const GOT_ENDED: &str =
    r#"{"type":"run_done","request":5,"code":4,"cancelled":false,"truncated":false}"#;
const GOT_CANCELLED: &str =
    r#"{"type":"run_done","request":5,"code":null,"cancelled":true,"truncated":false}"#;
const GOT_UNTRUSTED: &str = r#"{"type":"refused","request":5,"reason":{"type":"untrusted"}}"#;
const GOT_DENIED: &str =
    r#"{"type":"refused","request":5,"reason":{"type":"denied","capability":"process.run"}}"#;
const GOT_NOT_ALLOWED: &str = r#"{"type":"refused","request":5,"reason":{"type":"not_allowed","detail":"no `run` entry of its manifest allows `ls`"}}"#;
const TRUST: &str = r#"{"type":"trust","trusted":true}"#;
const DISTRUST: &str = r#"{"type":"trust","trusted":false}"#;

/// A plugin that runs `command` when it is sent the action `r`, and cancels it with `x`.
fn runner(command: &str) -> Echo {
    Echo::new(First::Echo)
        .asks_on_action_named(b'r', command)
        .asks_on_action_named(b'x', CANCEL)
}

fn patterns(texts: &[&str]) -> Vec<RunPattern> {
    texts
        .iter()
        .map(|text| RunPattern::parse(text).unwrap())
        .collect()
}

fn load(rig: &Rig, echo: Echo, grants: &[Capability], run: &[&str]) {
    rig.runtime.load_with(
        "tests",
        echo.build(),
        grants.iter().copied(),
        Thread::Shared,
        patterns(run),
    );
    heard(rig, "tests", "");
}

fn trusted_folder(rig: &Rig, trusted: bool) -> tempfile::TempDir {
    let folder = tempfile::tempdir().unwrap();
    rig.runtime
        .set_workspace(folder.path().to_path_buf(), trusted);
    folder
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only asks whether the process exists.
    unsafe { libc::kill(pid, 0) == 0 }
}

/// The id of the process the command printed, which is the one that sleeps.
fn pid_printed(rig: &Rig) -> i32 {
    let until = Instant::now() + Duration::from_secs(10);
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        let Ok(notice) = rig.notices.recv_timeout(left) else {
            break;
        };
        let Some(("tests", text)) = text_of(&notice) else {
            continue;
        };
        if let Some(rest) = text.split(r#""text":""#).nth(1) {
            return rest.trim_end_matches(r#"\n"}"#).parse().unwrap();
        }
    }
    panic!("the command printed nothing");
}

fn gone(pid: i32) -> bool {
    let until = Instant::now() + Duration::from_secs(10);
    while alive(pid) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    !alive(pid)
}

#[test]
fn a_plugin_runs_a_command_it_may_and_hears_its_output_and_its_end() {
    let rig = rig();
    let _folder = trusted_folder(&rig, true);
    load(
        &rig,
        runner(RUN_ECHO),
        &[Capability::ProcessRun],
        &["sh -c *"],
    );

    act(&rig, "tests", "r");

    // The two streams are read by threads of their own: in either order, but both before the end.
    let until = Instant::now() + Duration::from_secs(10);
    let mut told = Vec::new();
    while !told.iter().any(|text: &String| text == GOT_ENDED) {
        let left = until
            .checked_duration_since(Instant::now())
            .expect("the command never ended");
        let notice = rig
            .notices
            .recv_timeout(left)
            .expect("the command never ended");
        if let Some(("tests", text)) = text_of(&notice) {
            told.push(text.to_owned());
        }
    }
    assert!(told.iter().any(|text| text == GOT_HI), "{told:?}");
    assert!(told.iter().any(|text| text == GOT_OOPS), "{told:?}");
}

#[test]
fn a_command_that_no_pattern_allows_is_refused_and_does_not_run() {
    let rig = rig();
    let _folder = trusted_folder(&rig, true);
    load(
        &rig,
        runner(RUN_LS),
        &[Capability::ProcessRun],
        &["sh -c *"],
    );

    act(&rig, "tests", "r");

    heard(&rig, "tests", GOT_NOT_ALLOWED);
    never_heard(&rig, "tests", r#"{"type":"run_"#);
}

#[test]
fn a_plugin_without_the_capability_cannot_run_anything_whatever_it_lists() {
    let rig = rig();
    let _folder = trusted_folder(&rig, true);
    load(&rig, runner(RUN_ECHO), &[], &["sh -c *"]);

    act(&rig, "tests", "r");

    heard(&rig, "tests", GOT_DENIED);
}

#[test]
fn a_folder_that_is_not_trusted_refuses_and_asks_the_user_once_trusting_it_tells_the_plugin() {
    let rig = rig();
    let _folder = trusted_folder(&rig, false);
    load(
        &rig,
        runner(RUN_ECHO),
        &[Capability::ProcessRun],
        &["sh -c *"],
    );
    heard(&rig, "tests", DISTRUST);

    act(&rig, "tests", "r");

    // The app is told to ask before the plugin is told no.
    let mut asked = false;
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let left = until
            .checked_duration_since(Instant::now())
            .expect("it was refused");
        let notice = rig.notices.recv_timeout(left).expect("it was refused");
        asked |= notice
            == Notice::NeedsTrust {
                plugin: "tests".into(),
            };
        if matches!(text_of(&notice), Some(("tests", GOT_UNTRUSTED))) {
            break;
        }
    }
    assert!(asked, "the app was not told to ask");

    let folder = tempfile::tempdir().unwrap();
    rig.runtime.set_workspace(folder.path().to_path_buf(), true);
    heard(&rig, "tests", TRUST);
    act(&rig, "tests", "r");
    heard(&rig, "tests", GOT_ENDED);
}

#[test]
fn a_plugin_is_told_whether_the_folder_is_trusted_when_it_starts() {
    let rig = rig();
    let _folder = trusted_folder(&rig, true);

    load(
        &rig,
        runner(RUN_ECHO),
        &[Capability::ProcessRun],
        &["sh -c *"],
    );

    heard(&rig, "tests", TRUST);
}

#[test]
fn a_plugin_that_cannot_run_commands_is_not_told_about_trust() {
    let rig = rig();
    let _folder = trusted_folder(&rig, true);
    load(&rig, runner(RUN_ECHO), &[], &[]);

    rig.runtime.set_workspace(std::env::temp_dir(), false);

    never_heard(&rig, "tests", r#"{"type":"trust""#);
}

#[test]
fn a_command_that_cannot_start_is_refused_as_failed() {
    let rig = rig();
    let _folder = trusted_folder(&rig, true);
    load(
        &rig,
        runner(RUN_MISSING),
        &[Capability::ProcessRun],
        &["trcode-no-such-program"],
    );

    act(&rig, "tests", "r");

    heard_type(
        &rig,
        r#"{"type":"refused","request":5,"reason":{"type":"failed""#,
    );
}

/// Waits for text that starts with `prefix`.
fn heard_type(rig: &Rig, prefix: &str) {
    let until = Instant::now() + Duration::from_secs(10);
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        let Ok(notice) = rig.notices.recv_timeout(left) else {
            break;
        };
        if matches!(text_of(&notice), Some(("tests", text)) if text.starts_with(prefix)) {
            return;
        }
    }
    panic!("never heard {prefix}");
}

#[test]
fn a_cancel_stops_the_command_and_the_plugin_hears_that_it_was_cancelled() {
    let rig = rig();
    let _folder = trusted_folder(&rig, true);
    load(
        &rig,
        runner(RUN_SLEEP),
        &[Capability::ProcessRun],
        &["sh -c *"],
    );
    act(&rig, "tests", "r");
    let pid = pid_printed(&rig);

    act(&rig, "tests", "x");

    heard(&rig, "tests", GOT_CANCELLED);
    assert!(gone(pid));
}

#[test]
fn a_plugin_that_is_turned_off_has_its_commands_killed_and_hears_nothing() {
    let rig = rig();
    let _folder = trusted_folder(&rig, true);
    load(
        &rig,
        runner(RUN_SLEEP),
        &[Capability::ProcessRun],
        &["sh -c *"],
    );
    act(&rig, "tests", "r");
    let pid = pid_printed(&rig);

    rig.runtime.unload("tests");

    assert!(gone(pid));
    never_heard(&rig, "tests", r#"{"type":"run_done""#);
}

#[test]
fn a_plugin_that_traps_has_its_commands_killed_and_the_fresh_one_hears_nothing_of_them() {
    let rig = rig();
    let _folder = trusted_folder(&rig, true);
    load(
        &rig,
        runner(RUN_SLEEP).traps_on_refresh(),
        &[Capability::ProcessRun],
        &["sh -c *"],
    );
    act(&rig, "tests", "r");
    let pid = pid_printed(&rig);

    rig.runtime.send("tests", ToPlugin::Refresh);

    assert!(gone(pid));
    never_heard(&rig, "tests", r#"{"type":"run_done""#);
}
