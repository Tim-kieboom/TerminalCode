use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::policy::{Strikes, Verdict};
use super::view::node_of;
use super::{Notice, Runtime};
use crate::plugin_api::{ToPlugin, WireLine, WireView};
use crate::ui::view::{ViewLine, ViewNode};

// --- the plugins the tests run, written as WebAssembly text ---

const HELLO: &str = r#"{"type":"set_view","view":{"type":"lines","lines":[{"text":"hello from a plugin","slot":"explorer.file"}]}}"#;

fn escape(json: &str) -> String {
    json.replace('\\', "\\\\").replace('"', "\\\"")
}

fn assemble(text: String) -> Vec<u8> {
    wat::parse_str(text).expect("the test module is valid")
}

/// A plugin that sends each of `messages`, in order, whenever it is called.
fn sending(messages: &[&str]) -> Vec<u8> {
    let (mut data, mut calls, mut at) = (String::new(), String::new(), 0);
    for message in messages {
        data += &format!("(data (i32.const {at}) \"{}\")\n", escape(message));
        calls += &format!(
            "(call $send (i32.const {at}) (i32.const {}))\n",
            message.len()
        );
        at += message.len();
    }
    assemble(format!(
        r#"(module
            (import "host" "send" (func $send (param i32 i32)))
            (memory (export "memory") 2)
            {data}
            (func (export "alloc") (param i32) (result i32) (i32.const 65536))
            (func (export "handle") (param i32 i32) {calls}))"#
    ))
}

/// A plugin that never returns from `handle`.
fn looping() -> Vec<u8> {
    assemble(
        r#"(module
            (memory (export "memory") 1)
            (func (export "alloc") (param i32) (result i32) (i32.const 1024))
            (func (export "handle") (param i32 i32) (loop $forever (br $forever))))"#
            .into(),
    )
}

/// A plugin that never returns from its start function.
fn looping_at_start() -> Vec<u8> {
    assemble(
        r#"(module
            (memory (export "memory") 1)
            (func $spin (loop $forever (br $forever)))
            (start $spin)
            (func (export "alloc") (param i32) (result i32) (i32.const 1024))
            (func (export "handle") (param i32 i32)))"#
            .into(),
    )
}

/// A plugin that draws on its first call and loops forever on its second; a
/// fresh instance counts from the start again.
fn fails_on_the_second_call() -> Vec<u8> {
    let hello = escape(HELLO);
    assemble(format!(
        r#"(module
            (import "host" "send" (func $send (param i32 i32)))
            (memory (export "memory") 2)
            (data (i32.const 0) "{hello}")
            (global $calls (mut i32) (i32.const 0))
            (func (export "alloc") (param i32) (result i32) (i32.const 65536))
            (func (export "handle") (param i32 i32)
              (global.set $calls (i32.add (global.get $calls) (i32.const 1)))
              (if (i32.eq (global.get $calls) (i32.const 2))
                (then (loop $forever (br $forever))))
              (call $send (i32.const 0) (i32.const {len}))))"#,
        len = HELLO.len()
    ))
}

/// A plugin that asks for 64 MB, and says what it got.
fn asks_for_memory() -> Vec<u8> {
    let (granted, denied) = (
        escape(r#"{"type":"set_view","view":{"type":"lines","lines":[{"text":"granted"}]}}"#),
        escape(r#"{"type":"set_view","view":{"type":"lines","lines":[{"text":"denied"}]}}"#),
    );
    assemble(format!(
        r#"(module
            (import "host" "send" (func $send (param i32 i32)))
            (memory (export "memory") 2)
            (data (i32.const 0) "{granted}")
            (data (i32.const 512) "{denied}")
            (func (export "alloc") (param i32) (result i32) (i32.const 65536))
            (func (export "handle") (param i32 i32)
              (if (i32.eq (memory.grow (i32.const 1000)) (i32.const -1))
                (then (call $send (i32.const 512) (i32.const {denied_len})))
                (else (call $send (i32.const 0) (i32.const {granted_len}))))))"#,
        granted_len =
            r#"{"type":"set_view","view":{"type":"lines","lines":[{"text":"granted"}]}}"#.len(),
        denied_len =
            r#"{"type":"set_view","view":{"type":"lines","lines":[{"text":"denied"}]}}"#.len(),
    ))
}

/// A plugin that says it is sending far more than it may.
fn sends_too_much() -> Vec<u8> {
    assemble(
        r#"(module
            (import "host" "send" (func $send (param i32 i32)))
            (memory (export "memory") 1)
            (func (export "alloc") (param i32) (result i32) (i32.const 1024))
            (func (export "handle") (param i32 i32)
              (call $send (i32.const 0) (i32.const 2000000))))"#
            .into(),
    )
}

/// A plugin that has no `handle`.
fn without_handle() -> Vec<u8> {
    assemble(
        r#"(module
            (memory (export "memory") 1)
            (func (export "alloc") (param i32) (result i32) (i32.const 1024)))"#
            .into(),
    )
}

// --- running them ---

struct Rig {
    runtime: Runtime,
    notices: mpsc::Receiver<Notice>,
}

fn rig() -> Rig {
    let (sender, notices) = mpsc::channel();
    Rig {
        runtime: Runtime::spawn(move |notice| {
            let _ = sender.send(notice);
        }),
        notices,
    }
}

impl Rig {
    fn next(&self) -> Notice {
        self.notices
            .recv_timeout(Duration::from_secs(30))
            .expect("the runtime said something")
    }

    fn nothing_more(&self) {
        assert_eq!(
            self.notices.recv_timeout(Duration::from_millis(300)),
            Err(mpsc::RecvTimeoutError::Timeout)
        );
    }
}

fn lines(text: &str, slot: &str) -> ViewNode {
    ViewNode::Lines(vec![ViewLine {
        text: text.into(),
        slot: slot.into(),
    }])
}

#[test]
fn a_plugin_draws_when_it_is_loaded() {
    let rig = rig();

    rig.runtime.load("hello", sending(&[HELLO]));

    assert_eq!(
        rig.next(),
        Notice::View {
            plugin: "hello".into(),
            content: lines("hello from a plugin", "explorer.file"),
        }
    );
    rig.nothing_more();
}

#[test]
fn a_plugin_that_loops_is_trapped_restarted_and_then_disabled() {
    let rig = rig();
    let started = Instant::now();

    rig.runtime.load("looper", looping());

    let reasons: Vec<_> = (0..3).map(|_| rig.next()).collect();
    let restarted = |reason: &str| Notice::Restarted {
        plugin: "looper".into(),
        reason: reason.into(),
    };
    assert_eq!(reasons[0], restarted("it ran out of budget"));
    assert_eq!(reasons[1], restarted("it ran out of budget"));
    assert_eq!(
        reasons[2],
        Notice::Disabled {
            plugin: "looper".into(),
            reason: "it ran out of budget".into(),
        }
    );
    rig.nothing_more();
    assert!(
        started.elapsed() < Duration::from_secs(25),
        "the budget stops it quickly"
    );
}

#[test]
fn a_plugin_that_loops_while_starting_is_disabled_too() {
    let rig = rig();

    rig.runtime.load("spinner", looping_at_start());

    assert!(matches!(rig.next(), Notice::Restarted { .. }));
    assert!(matches!(rig.next(), Notice::Restarted { .. }));
    assert!(matches!(rig.next(), Notice::Disabled { .. }));
}

#[test]
fn a_disabled_plugin_hears_nothing_more() {
    let rig = rig();
    rig.runtime.load("looper", looping());
    for _ in 0..3 {
        rig.next();
    }

    rig.runtime.send("looper", ToPlugin::Refresh);

    rig.nothing_more();
}

#[test]
fn a_restarted_plugin_is_fresh_and_draws_again() {
    let rig = rig();
    rig.runtime.load("flaky", fails_on_the_second_call());
    assert!(matches!(rig.next(), Notice::View { .. }), "the first call");

    rig.runtime.send("flaky", ToPlugin::Refresh);

    assert!(matches!(rig.next(), Notice::Restarted { .. }), "the second");
    // The fresh instance counts its first call again, so it draws.
    assert!(matches!(rig.next(), Notice::View { .. }));
    rig.nothing_more();
}

#[test]
fn the_memory_of_a_plugin_is_limited() {
    let rig = rig();

    rig.runtime.load("greedy", asks_for_memory());

    assert_eq!(
        rig.next(),
        Notice::View {
            plugin: "greedy".into(),
            content: lines("denied", ""),
        }
    );
}

#[test]
fn a_message_that_is_too_big_counts_against_the_plugin() {
    let rig = rig();

    rig.runtime.load("loud", sends_too_much());

    let first = rig.next();
    let Notice::Restarted { reason, .. } = first else {
        panic!("{first:?}");
    };
    assert!(reason.contains("more than"), "{reason}");
    assert!(matches!(rig.next(), Notice::Restarted { .. }));
    assert!(matches!(rig.next(), Notice::Disabled { .. }));
}

#[test]
fn a_plugin_that_cannot_run_is_not_started_again() {
    let rig = rig();

    rig.runtime
        .load("garbage", b"this is not WebAssembly".to_vec());
    rig.runtime.load("headless", without_handle());

    let first = rig.next();
    assert!(
        matches!(&first, Notice::Broken { plugin, reason }
            if &**plugin == "garbage" && reason.contains("not a valid module")),
        "{first:?}"
    );
    let second = rig.next();
    assert!(
        matches!(&second, Notice::Broken { plugin, reason }
            if &**plugin == "headless" && reason.contains("handle")),
        "{second:?}"
    );
    rig.nothing_more();
}

#[test]
fn what_a_plugin_says_that_is_not_understood_is_ignored_and_told_once() {
    let rig = rig();
    let unknown_kind = r#"{"type":"teleport","to":"mars"}"#;
    let unknown_view = r#"{"type":"set_view","view":{"type":"hologram"}}"#;

    rig.runtime.load(
        "odd",
        sending(&[unknown_kind, unknown_view, "nope", "also nope"]),
    );

    let first = rig.next();
    assert!(
        matches!(&first, Notice::View { content, .. }
            if *content == node_of(WireView::Unknown)),
        "an unknown view is drawn as a note: {first:?}"
    );
    assert!(matches!(rig.next(), Notice::Rejected { .. }), "told once");
    rig.nothing_more();
}

#[test]
fn a_plugin_with_the_same_name_replaces_the_one_before() {
    let rig = rig();
    rig.runtime.load("a", sending(&[HELLO]));
    rig.next();

    rig.runtime.load("a", looping());

    assert!(matches!(rig.next(), Notice::Restarted { .. }));
}

// --- what to do about traps ---

#[test]
fn the_third_trap_within_a_minute_disables() {
    let start = Instant::now();
    let mut strikes = Strikes::default();

    assert_eq!(strikes.record(start), Verdict::Restart);
    assert_eq!(
        strikes.record(start + Duration::from_secs(10)),
        Verdict::Restart
    );
    assert_eq!(
        strikes.record(start + Duration::from_secs(20)),
        Verdict::Disable
    );
}

#[test]
fn a_quiet_minute_wipes_the_count() {
    let start = Instant::now();
    let mut strikes = Strikes::default();
    strikes.record(start);
    strikes.record(start + Duration::from_secs(1));

    let later = start + Duration::from_secs(62);

    assert_eq!(strikes.record(later), Verdict::Restart);
    assert_eq!(
        strikes.record(later + Duration::from_secs(1)),
        Verdict::Restart
    );
}

#[test]
fn traps_that_are_spread_out_never_disable() {
    let start = Instant::now();
    let mut strikes = Strikes::default();

    for minute in 0..10 {
        let at = start + Duration::from_secs(minute * 61);
        assert_eq!(strikes.record(at), Verdict::Restart);
    }
}

// --- from a message to what the pane draws ---

#[test]
fn lines_become_lines_with_their_slots() {
    let view = WireView::lines([WireLine::new("one", "a.slot"), WireLine::new("two", "")]);

    assert_eq!(
        node_of(view),
        ViewNode::Lines(vec![
            ViewLine {
                text: "one".into(),
                slot: "a.slot".into()
            },
            ViewLine {
                text: "two".into(),
                slot: "".into()
            },
        ])
    );
}

#[test]
fn a_list_becomes_a_list_with_its_selection() {
    let view = WireView::list(["a".to_owned(), "b".to_owned()], Some(1));

    assert_eq!(
        node_of(view),
        ViewNode::List {
            items: vec!["a".into(), "b".into()],
            selected: Some(1)
        }
    );
}

#[test]
fn a_view_this_version_cannot_draw_is_a_line_that_says_so() {
    let ViewNode::Lines(lines) = node_of(WireView::Unknown) else {
        panic!("a line of text");
    };

    assert_eq!(lines.len(), 1);
    assert!(lines[0].text.contains("cannot show"));
}
