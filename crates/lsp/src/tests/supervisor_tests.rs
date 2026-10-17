use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::supervisor::{MAX_RESTARTS, RESTART_WINDOW};
use crate::{Effect, LaunchSpec, LspError, Notice, ServerKey, StartError, Supervisor};

fn key() -> ServerKey {
    ServerKey {
        name: "rust".to_owned(),
        root: PathBuf::from("/p"),
    }
}

fn spec() -> LaunchSpec {
    LaunchSpec {
        name: "rust".to_owned(),
        command: "rust-analyzer".to_owned(),
        args: Vec::new(),
        root: PathBuf::from("/p"),
        initialization_options: Value::Null,
    }
}

fn at(seconds: u64, base: Instant) -> Instant {
    base + Duration::from_secs(seconds)
}

/// A supervisor with the server running.
fn running() -> Supervisor {
    let mut supervisor = Supervisor::default();
    supervisor.ensure(&key(), spec());
    supervisor.launched(&key(), Ok(()));
    supervisor
}

fn notices(effects: &[Effect]) -> Vec<&Notice> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Notify(notice) => Some(notice),
            Effect::Launch(..) => None,
        })
        .collect()
}

fn launches(effects: &[Effect]) -> usize {
    effects
        .iter()
        .filter(|effect| matches!(effect, Effect::Launch(..)))
        .count()
}

fn spawn_failure(reason: &str) -> StartError {
    StartError::Spawn(reason.to_owned())
}

// ---- starting

#[test]
fn the_first_file_that_wants_a_server_starts_it_and_the_next_ones_do_not() {
    let mut supervisor = Supervisor::default();

    let first = supervisor.ensure(&key(), spec());
    let second = supervisor.ensure(&key(), spec());

    assert_eq!(first, vec![Effect::Launch(key(), spec())]);
    assert_eq!(second, vec![]);
    assert!(supervisor.is_starting(&key()));
}

#[test]
fn a_server_that_started_is_running_and_nothing_is_said() {
    let mut supervisor = Supervisor::default();
    supervisor.ensure(&key(), spec());

    let effects = supervisor.launched(&key(), Ok(()));

    assert_eq!(effects, vec![]);
    assert!(supervisor.is_running(&key()));
    assert_eq!(supervisor.status(&key()).as_deref(), Some("rust-analyzer"));
}

#[test]
fn a_server_that_cannot_start_is_reported_once_as_an_error_with_the_reason() {
    let mut supervisor = Supervisor::default();
    supervisor.ensure(&key(), spec());

    let effects = supervisor.launched(&key(), Err(spawn_failure("`rust-analyzer` was not found")));

    let notices = notices(&effects);
    assert_eq!(notices.len(), 1);
    assert!(notices[0].error, "an error stays on screen");
    assert!(
        notices[0].text.contains("rust-analyzer could not start"),
        "{}",
        notices[0].text
    );
    assert!(
        notices[0].text.contains("was not found"),
        "{}",
        notices[0].text
    );
    assert!(
        notices[0].text.contains("not tried again"),
        "{}",
        notices[0].text
    );
    assert_eq!(
        supervisor.status(&key()).as_deref(),
        Some("rust-analyzer: off")
    );
}

#[test]
fn a_server_that_exited_at_once_says_what_it_wrote_to_stderr() {
    let mut supervisor = Supervisor::default();
    supervisor.ensure(&key(), spec());
    let error = StartError::Handshake {
        error: LspError::Closed("the server ended the connection".to_owned()),
        stderr: Some("error: Unknown binary 'rust-analyzer' in official toolchain".to_owned()),
    };

    let effects = supervisor.launched(&key(), Err(error));

    assert!(
        notices(&effects)[0]
            .text
            .contains("Unknown binary 'rust-analyzer'"),
        "{effects:?}"
    );
}

#[test]
fn a_server_that_is_off_is_not_started_again_by_another_file() {
    let mut supervisor = Supervisor::default();
    supervisor.ensure(&key(), spec());
    supervisor.launched(&key(), Err(spawn_failure("gone")));

    let effects = supervisor.ensure(&key(), spec());

    assert_eq!(effects, vec![], "told once, not on every file");
}

#[test]
fn a_reset_lets_the_next_file_start_the_server_again() {
    let mut supervisor = Supervisor::default();
    supervisor.ensure(&key(), spec());
    supervisor.launched(&key(), Err(spawn_failure("gone")));

    supervisor.reset();
    let effects = supervisor.ensure(&key(), spec());

    assert_eq!(launches(&effects), 1);
    assert_eq!(
        supervisor.status(&key()).as_deref(),
        Some("rust-analyzer: starting")
    );
}

#[test]
fn a_server_never_asked_for_has_no_status() {
    assert_eq!(Supervisor::default().status(&key()), None);
}

#[test]
fn the_result_of_a_start_nobody_wants_is_ignored() {
    let mut supervisor = Supervisor::default();

    assert_eq!(supervisor.launched(&key(), Ok(())), vec![]);
    assert_eq!(supervisor.launched(&key(), Err(spawn_failure("x"))), vec![]);
    supervisor.ensure(&key(), spec());
    supervisor.reset();
    assert_eq!(supervisor.launched(&key(), Err(spawn_failure("x"))), vec![]);
    assert!(!supervisor.is_running(&key()));
}

// ---- dying and restarting

#[test]
fn a_server_that_dies_is_restarted_with_a_notice_that_counts() {
    let mut supervisor = running();
    let now = Instant::now();

    let effects = supervisor.closed(&key(), "the server ended the connection", now);

    let notices = notices(&effects);
    assert_eq!(notices.len(), 1);
    assert!(!notices[0].error, "a restart is news, not an error");
    assert!(
        notices[0].text.contains("restarting it (1 of 3)"),
        "{}",
        notices[0].text
    );
    assert!(
        notices[0].text.contains("ended the connection"),
        "{}",
        notices[0].text
    );
    assert_eq!(launches(&effects), 1);
    assert_eq!(
        supervisor.status(&key()).as_deref(),
        Some("rust-analyzer: starting")
    );
}

#[test]
fn it_is_restarted_three_times_and_the_fourth_death_leaves_it_off() {
    let mut supervisor = running();
    let base = Instant::now();

    for attempt in 1..=MAX_RESTARTS {
        let effects = supervisor.closed(&key(), "crashed", at(attempt as u64, base));
        assert_eq!(launches(&effects), 1, "restart {attempt}");
        assert!(
            notices(&effects)[0]
                .text
                .contains(&format!("({attempt} of {MAX_RESTARTS})")),
            "{effects:?}"
        );
        supervisor.launched(&key(), Ok(()));
    }
    let effects = supervisor.closed(&key(), "crashed", at(10, base));

    assert_eq!(launches(&effects), 0);
    let notices = notices(&effects);
    assert!(notices[0].error);
    assert!(notices[0].text.contains("stays off"), "{}", notices[0].text);
    assert!(
        notices[0].text.contains("restarted 3 times"),
        "{}",
        notices[0].text
    );
    assert!(!supervisor.is_running(&key()));
    assert_eq!(
        supervisor.status(&key()).as_deref(),
        Some("rust-analyzer: off")
    );
}

#[test]
fn deaths_older_than_the_window_do_not_count() {
    let mut supervisor = running();
    let base = Instant::now();
    for attempt in 0..MAX_RESTARTS {
        supervisor.closed(&key(), "crashed", at(attempt as u64, base));
        supervisor.launched(&key(), Ok(()));
    }

    let later = base + RESTART_WINDOW + Duration::from_secs(10);
    let effects = supervisor.closed(&key(), "crashed", later);

    assert_eq!(
        launches(&effects),
        1,
        "a server that ran fine for a minute gets its restarts back"
    );
    assert!(
        notices(&effects)[0].text.contains("(1 of 3)"),
        "{effects:?}"
    );
}

#[test]
fn a_restart_that_cannot_start_leaves_it_off_with_the_reason() {
    let mut supervisor = running();
    supervisor.closed(&key(), "crashed", Instant::now());

    let effects = supervisor.launched(&key(), Err(spawn_failure("the program was deleted")));

    let notices = notices(&effects);
    assert!(notices[0].error);
    assert!(
        notices[0].text.contains("the program was deleted"),
        "{}",
        notices[0].text
    );
    assert_eq!(
        supervisor.closed(&key(), "again", Instant::now()),
        vec![],
        "off stays off"
    );
}

#[test]
fn a_connection_that_ends_before_the_server_ran_is_not_a_death() {
    let mut supervisor = Supervisor::default();
    supervisor.ensure(&key(), spec());

    assert_eq!(supervisor.closed(&key(), "early", Instant::now()), vec![]);
    assert!(supervisor.is_starting(&key()));
}

#[test]
fn a_connection_that_ends_for_a_server_that_is_off_or_unknown_does_nothing() {
    let mut supervisor = Supervisor::default();

    assert_eq!(supervisor.closed(&key(), "x", Instant::now()), vec![]);
    supervisor.ensure(&key(), spec());
    supervisor.launched(&key(), Err(spawn_failure("x")));
    assert_eq!(supervisor.closed(&key(), "x", Instant::now()), vec![]);
}

// ---- progress

#[test]
fn what_the_server_reports_shows_in_the_status_until_it_is_done() {
    let mut supervisor = running();

    supervisor.notification(
        &key(),
        "$/progress",
        &json!({ "token": "t", "value": { "kind": "begin", "title": "Indexing", "percentage": 3 } }),
    );
    assert_eq!(
        supervisor.status(&key()).as_deref(),
        Some("rust-analyzer: Indexing 3%")
    );

    supervisor.notification(
        &key(),
        "$/progress",
        &json!({ "token": "t", "value": { "kind": "end" } }),
    );
    assert_eq!(supervisor.status(&key()).as_deref(), Some("rust-analyzer"));
}

#[test]
fn other_notifications_and_unknown_servers_are_ignored() {
    let mut supervisor = running();

    supervisor.notification(&key(), "window/logMessage", &json!({ "message": "hi" }));
    supervisor.notification(
        &ServerKey {
            name: "other".to_owned(),
            root: PathBuf::from("/q"),
        },
        "$/progress",
        &json!({ "token": "t", "value": { "kind": "begin", "title": "x" } }),
    );

    assert_eq!(supervisor.status(&key()).as_deref(), Some("rust-analyzer"));
}

#[test]
fn a_restart_starts_with_no_progress_from_the_one_that_died() {
    let mut supervisor = running();
    supervisor.notification(
        &key(),
        "$/progress",
        &json!({ "token": "t", "value": { "kind": "begin", "title": "Indexing" } }),
    );
    supervisor.closed(&key(), "crashed", Instant::now());
    supervisor.launched(&key(), Ok(()));

    assert_eq!(supervisor.status(&key()).as_deref(), Some("rust-analyzer"));
}
