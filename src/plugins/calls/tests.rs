use super::{Calls, Outgoing};
use crate::plugin_api::{MAX_OPEN_CALLS, Refusal, ToPlugin};

fn serving(plugins: &[&str]) -> Calls {
    let mut calls = Calls::default();
    for plugin in plugins {
        calls.serve(plugin);
    }
    calls
}

fn open(calls: &mut Calls, caller: &str, request: u64, to: &str) -> Result<Outgoing, Refusal> {
    calls.open(caller, request, to, "m".into(), "p".into())
}

fn reply(calls: &mut Calls, from: &str, to: &str, request: u64, last: bool) -> Option<Outgoing> {
    calls.reply(from, to, request, "x".into(), last)
}

fn refused(to: &str, request: u64) -> Outgoing {
    Outgoing {
        to: to.into(),
        message: ToPlugin::Refused {
            request,
            reason: Refusal::Gone,
        },
    }
}

fn cancelled(to: &str, from: &str, request: u64) -> Outgoing {
    Outgoing {
        to: to.into(),
        message: ToPlugin::CallCancelled {
            from: from.into(),
            request,
        },
    }
}

#[test]
fn a_call_goes_to_the_callee_with_the_name_of_the_caller() {
    let mut calls = serving(&["rust"]);

    let sent = open(&mut calls, "tests", 3, "rust").unwrap();

    assert_eq!(&*sent.to, "rust");
    assert_eq!(
        sent.message,
        ToPlugin::Call {
            from: "tests".into(),
            request: 3,
            method: "m".into(),
            payload: "p".into(),
        }
    );
}

#[test]
fn a_plugin_that_is_not_there_or_does_not_serve_cannot_be_called() {
    let mut calls = serving(&["rust"]);

    assert_eq!(
        open(&mut calls, "tests", 1, "python"),
        Err(Refusal::NoSuchPlugin)
    );
}

#[test]
fn a_plugin_cannot_call_itself() {
    let mut calls = serving(&["rust"]);

    assert!(matches!(
        open(&mut calls, "rust", 1, "rust"),
        Err(Refusal::BadArgument { .. })
    ));
}

#[test]
fn a_request_number_that_is_open_is_refused_and_a_free_one_is_not() {
    let mut calls = serving(&["rust"]);
    open(&mut calls, "tests", 1, "rust").unwrap();

    assert!(matches!(
        open(&mut calls, "tests", 1, "rust"),
        Err(Refusal::BadArgument { .. })
    ));
    assert!(open(&mut calls, "tests", 2, "rust").is_ok());
    assert!(
        open(&mut calls, "other", 1, "rust").is_ok(),
        "the numbers are the caller's own"
    );
}

#[test]
fn a_caller_has_at_most_so_many_calls_open_and_an_ended_call_makes_room() {
    let mut calls = serving(&["rust"]);
    for request in 0..MAX_OPEN_CALLS as u64 {
        open(&mut calls, "tests", request, "rust").unwrap();
    }

    assert_eq!(open(&mut calls, "tests", 1000, "rust"), Err(Refusal::Busy));
    assert!(
        open(&mut calls, "other", 1000, "rust").is_ok(),
        "the limit is for each caller"
    );
    reply(&mut calls, "rust", "tests", 0, true).unwrap();
    assert!(open(&mut calls, "tests", 1000, "rust").is_ok());
}

#[test]
fn parts_of_an_answer_reach_the_caller_until_the_last_ends_the_call() {
    let mut calls = serving(&["rust"]);
    open(&mut calls, "tests", 3, "rust").unwrap();

    let first = reply(&mut calls, "rust", "tests", 3, false).unwrap();
    assert_eq!(&*first.to, "tests");
    assert_eq!(
        first.message,
        ToPlugin::Reply {
            from: "rust".into(),
            request: 3,
            payload: "x".into(),
            last: false,
        }
    );
    assert!(reply(&mut calls, "rust", "tests", 3, true).is_some());
    assert_eq!(
        reply(&mut calls, "rust", "tests", 3, false),
        None,
        "it ended"
    );
}

#[test]
fn an_answer_from_a_plugin_the_call_was_not_made_to_is_dropped() {
    let mut calls = serving(&["rust", "python"]);
    open(&mut calls, "tests", 3, "rust").unwrap();

    assert_eq!(reply(&mut calls, "python", "tests", 3, true), None);
    assert!(
        reply(&mut calls, "rust", "tests", 3, true).is_some(),
        "the call is still open"
    );
}

#[test]
fn an_answer_to_a_call_nobody_made_is_dropped() {
    let mut calls = serving(&["rust"]);

    assert_eq!(reply(&mut calls, "rust", "tests", 3, true), None);
}

#[test]
fn a_cancel_tells_the_callee_and_drops_what_it_says_after() {
    let mut calls = serving(&["rust"]);
    open(&mut calls, "tests", 3, "rust").unwrap();

    assert_eq!(
        calls.cancel("tests", 3),
        Some(cancelled("rust", "tests", 3))
    );
    assert_eq!(reply(&mut calls, "rust", "tests", 3, false), None);
    assert_eq!(calls.cancel("tests", 3), None, "it is not open any more");
    assert!(
        open(&mut calls, "tests", 3, "rust").is_ok(),
        "the number is free again"
    );
}

#[test]
fn a_callee_that_is_reset_fails_the_calls_it_was_answering_and_can_be_called_again() {
    let mut calls = serving(&["rust"]);
    open(&mut calls, "tests", 2, "rust").unwrap();
    open(&mut calls, "tests", 1, "rust").unwrap();
    open(&mut calls, "other", 5, "rust").unwrap();

    let told = calls.reset("rust");

    assert_eq!(
        told,
        vec![
            refused("other", 5),
            refused("tests", 1),
            refused("tests", 2)
        ]
    );
    assert_eq!(reply(&mut calls, "rust", "tests", 1, true), None);
    assert!(open(&mut calls, "tests", 1, "rust").is_ok());
}

#[test]
fn a_caller_that_is_reset_cancels_the_calls_it_made() {
    let mut calls = serving(&["rust", "python"]);
    open(&mut calls, "tests", 2, "python").unwrap();
    open(&mut calls, "tests", 1, "rust").unwrap();

    let told = calls.reset("tests");

    assert_eq!(
        told,
        vec![
            cancelled("rust", "tests", 1),
            cancelled("python", "tests", 2)
        ]
    );
    assert_eq!(reply(&mut calls, "rust", "tests", 1, true), None);
}

#[test]
fn a_plugin_that_is_removed_fails_its_calls_and_cannot_be_called() {
    let mut calls = serving(&["rust"]);
    open(&mut calls, "tests", 1, "rust").unwrap();

    assert_eq!(calls.remove("rust"), vec![refused("tests", 1)]);
    assert_eq!(
        open(&mut calls, "tests", 2, "rust"),
        Err(Refusal::NoSuchPlugin)
    );
}

#[test]
fn a_call_that_cannot_be_delivered_is_failed_once() {
    let mut calls = serving(&["rust"]);
    open(&mut calls, "tests", 1, "rust").unwrap();

    assert_eq!(calls.abandon("tests", 1), Some(refused("tests", 1)));
    assert_eq!(calls.abandon("tests", 1), None);
}
