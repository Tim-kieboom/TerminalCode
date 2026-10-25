//! Plugins calling plugins, through the runtime and real modules: the call, its
//! answer in parts, the cancel, and what happens when either plugin is lost.

use std::time::{Duration, Instant};

use super::super::fixtures::{ACTION_AT, Echo, First, TYPE_AT};
use super::{Rig, rig};
use crate::plugin_api::{Capability, ToPlugin};
use crate::plugins::calls::{Calls, Outgoing};
use crate::plugins::{Notice, Thread};
use crate::ui::view::ViewNode;

use crate::plugin_api::{MAX_OPEN_CALLS, Refusal};

const CALL: &str =
    r#"{"type":"call","request":3,"to":"adapter","method":"discover","payload":"p"}"#;
const CANCEL: &str = r#"{"type":"call_cancel","request":3}"#;
const PART: &str = r#"{"type":"reply","to":"tests","request":3,"payload":"part"}"#;
const LAST: &str = r#"{"type":"reply","to":"tests","request":3,"payload":"done","last":true}"#;

/// The place of the quote that ends `call` in `{"type":"call","from":...`; in
/// `call_cancelled` there is an underscore there.
const AFTER_CALL: u32 = TYPE_AT + 4;

const GOT_CALL: &str =
    r#"{"type":"call","from":"tests","request":3,"method":"discover","payload":"p"}"#;
const GOT_PART: &str =
    r#"{"type":"reply","from":"adapter","request":3,"payload":"part","last":false}"#;
const GOT_LAST: &str =
    r#"{"type":"reply","from":"adapter","request":3,"payload":"done","last":true}"#;
const GOT_CANCELLED: &str = r#"{"type":"call_cancelled","from":"tests","request":3}"#;
const GOT_GONE: &str = r#"{"type":"refused","request":3,"reason":{"type":"gone"}}"#;
const GOT_NO_SUCH: &str = r#"{"type":"refused","request":3,"reason":{"type":"no_such_plugin"}}"#;
const GOT_DENIED: &str =
    r#"{"type":"refused","request":3,"reason":{"type":"denied","capability":"plugin.call"}}"#;

/// A plugin that calls the adapter when it is sent the action `c`, and cancels the call
/// with `x`.
fn caller() -> Echo {
    Echo::new(First::Echo)
        .asks_on_action_named(b'c', CALL)
        .asks_on_action_named(b'x', CANCEL)
}

/// A plugin that answers a call in two parts.
fn answering_adapter() -> Echo {
    Echo::new(First::Echo)
        .asks_on_byte(AFTER_CALL, b'"', PART)
        .asks_on_byte(AFTER_CALL, b'"', LAST)
}

/// A plugin that never answers; told that the call was cancelled, it still says a part.
fn adapter_that_answers_too_late() -> Echo {
    Echo::new(First::Echo).asks_on_byte(AFTER_CALL, b'_', PART)
}

pub(super) fn load(rig: &Rig, plugin: &str, echo: Echo, grants: &[Capability], thread: Thread) {
    rig.runtime
        .load_on(plugin, echo.build(), grants.iter().copied(), thread);
    // Its first view, which says it started.
    heard(rig, plugin, "");
}

fn call_plugin(rig: &Rig, thread: Thread) {
    load(rig, "tests", caller(), &[Capability::PluginCall], thread);
}

fn serve_plugin(rig: &Rig, echo: Echo, thread: Thread) {
    load(rig, "adapter", echo, &[Capability::PluginServe], thread);
}

pub(super) fn text_of(notice: &Notice) -> Option<(&str, &str)> {
    match notice {
        Notice::View {
            plugin,
            content: ViewNode::Lines(lines),
        } => Some((plugin, &lines[0].text)),
        _ => None,
    }
}

/// Waits for `plugin` to show text that is `exactly` what it was sent (anything when
/// empty); what else happens in between is passed over.
pub(super) fn heard(rig: &Rig, plugin: &str, exactly: &str) {
    let until = Instant::now() + Duration::from_secs(10);
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        let Ok(notice) = rig.notices.recv_timeout(left) else {
            break;
        };
        if let Some((from, text)) = text_of(&notice)
            && from == plugin
            && (exactly.is_empty() || text == exactly)
        {
            return;
        }
    }
    panic!("{plugin} never heard {exactly}");
}

/// That `plugin` is not sent anything in the next half second that starts with `prefix`.
pub(super) fn never_heard(rig: &Rig, plugin: &str, prefix: &str) {
    let until = Instant::now() + Duration::from_millis(500);
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        let Ok(notice) = rig.notices.recv_timeout(left) else {
            return;
        };
        if let Some((from, text)) = text_of(&notice) {
            assert!(
                from != plugin || !text.starts_with(prefix),
                "{plugin} heard {text}"
            );
        }
    }
}

pub(super) fn act(rig: &Rig, plugin: &str, action: &str) {
    rig.runtime.send(plugin, ToPlugin::action(action));
    heard(
        rig,
        plugin,
        &format!(r#"{{"type":"action","action":"{action}"}}"#),
    );
}

fn a_call_is_answered_in_parts(caller_on: Thread, adapter_on: Thread) {
    let rig = rig();
    serve_plugin(&rig, answering_adapter(), adapter_on);
    call_plugin(&rig, caller_on);

    act(&rig, "tests", "c");

    heard(&rig, "adapter", GOT_CALL);
    heard(&rig, "tests", GOT_PART);
    heard(&rig, "tests", GOT_LAST);
}

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

#[test]
fn a_call_is_answered_in_parts_on_the_shared_thread() {
    a_call_is_answered_in_parts(Thread::Shared, Thread::Shared);
}

#[test]
fn a_call_is_answered_in_parts_across_threads() {
    a_call_is_answered_in_parts(Thread::Shared, Thread::Own);
    a_call_is_answered_in_parts(Thread::Own, Thread::Shared);
    a_call_is_answered_in_parts(Thread::Own, Thread::Own);
}

#[test]
fn a_plugin_without_the_capability_cannot_call() {
    let rig = rig();
    serve_plugin(&rig, answering_adapter(), Thread::Shared);
    load(&rig, "tests", caller(), &[], Thread::Shared);

    act(&rig, "tests", "c");

    heard(&rig, "tests", GOT_DENIED);
    never_heard(&rig, "adapter", r#"{"type":"call""#);
}

#[test]
fn a_call_to_a_plugin_that_is_not_there_is_refused() {
    let rig = rig();
    call_plugin(&rig, Thread::Shared);

    act(&rig, "tests", "c");

    heard(&rig, "tests", GOT_NO_SUCH);
}

#[test]
fn a_call_to_a_plugin_that_does_not_serve_is_refused_and_it_is_not_called() {
    let rig = rig();
    load(&rig, "adapter", answering_adapter(), &[], Thread::Shared);
    call_plugin(&rig, Thread::Shared);

    act(&rig, "tests", "c");

    heard(&rig, "tests", GOT_NO_SUCH);
    never_heard(&rig, "adapter", r#"{"type":"call""#);
}

#[test]
fn a_cancel_tells_the_other_plugin_and_what_it_says_after_never_arrives() {
    let rig = rig();
    serve_plugin(&rig, adapter_that_answers_too_late(), Thread::Own);
    call_plugin(&rig, Thread::Shared);
    act(&rig, "tests", "c");
    heard(&rig, "adapter", GOT_CALL);

    act(&rig, "tests", "x");

    heard(&rig, "adapter", GOT_CANCELLED);
    never_heard(&rig, "tests", r#"{"type":"reply""#);
}

#[test]
fn a_plugin_that_traps_while_it_is_called_fails_the_call() {
    let rig = rig();
    let trapping = Echo::new(First::Echo).traps_on_byte(AFTER_CALL, b'"');
    serve_plugin(&rig, trapping, Thread::Shared);
    call_plugin(&rig, Thread::Shared);

    act(&rig, "tests", "c");

    heard(&rig, "tests", GOT_GONE);
}

#[test]
fn a_caller_that_traps_cancels_its_calls() {
    let rig = rig();
    serve_plugin(&rig, adapter_that_answers_too_late(), Thread::Shared);
    load(
        &rig,
        "tests",
        caller().traps_on_refresh(),
        &[Capability::PluginCall],
        Thread::Shared,
    );
    act(&rig, "tests", "c");
    heard(&rig, "adapter", GOT_CALL);

    rig.runtime.send("tests", ToPlugin::Refresh);

    heard(&rig, "adapter", GOT_CANCELLED);
    never_heard(&rig, "tests", r#"{"type":"reply""#);
}

#[test]
fn a_plugin_that_is_turned_off_fails_the_calls_it_had() {
    let rig = rig();
    serve_plugin(&rig, adapter_that_answers_too_late(), Thread::Own);
    call_plugin(&rig, Thread::Shared);
    act(&rig, "tests", "c");
    heard(&rig, "adapter", GOT_CALL);

    rig.runtime.unload("adapter");

    heard(&rig, "tests", GOT_GONE);
}

#[test]
fn a_plugin_that_is_replaced_fails_the_calls_it_had_and_is_called_afresh() {
    let rig = rig();
    serve_plugin(&rig, adapter_that_answers_too_late(), Thread::Shared);
    call_plugin(&rig, Thread::Shared);
    act(&rig, "tests", "c");
    heard(&rig, "adapter", GOT_CALL);

    rig.runtime.load_on(
        "adapter",
        answering_adapter().build(),
        [Capability::PluginServe],
        Thread::Shared,
    );

    // The caller is told before the new plugin has started.
    heard(&rig, "tests", GOT_GONE);
    heard(&rig, "adapter", "");
    act(&rig, "tests", "c");
    heard(&rig, "tests", GOT_LAST);
}

#[test]
fn a_plugin_that_cannot_run_cannot_be_called() {
    let rig = rig();
    rig.runtime.load(
        "adapter",
        super::without_handle(),
        [Capability::PluginServe],
    );
    loop {
        if matches!(rig.next(), Notice::Broken { .. }) {
            break;
        }
    }
    call_plugin(&rig, Thread::Shared);

    act(&rig, "tests", "c");

    heard(&rig, "tests", GOT_NO_SUCH);
}

#[test]
fn the_action_and_type_places_are_where_the_fixtures_look() {
    assert_eq!(r#"{"type":""#.len() as u32, TYPE_AT);
    assert_eq!(r#"{"type":"action","action":""#.len() as u32, ACTION_AT);
    assert_eq!(&GOT_CALL[AFTER_CALL as usize..][..1], "\"");
    assert_eq!(&GOT_CANCELLED[AFTER_CALL as usize..][..1], "_");
}
