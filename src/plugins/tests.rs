mod calls;
mod menu;
mod runs;

use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::policy::{Strikes, Verdict};
use super::runtime::fuel_for;
use super::view::node_of;
use super::{CALLBACK_FUEL, FUEL_PER_BYTE, Notice, OWNED_FUEL_CEILING, Runtime, Thread};
use crate::plugin_api::{Capability, ToPlugin, WireLine, WireView};
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
              (call $send (i32.const 0) (i32.const 5000000))))"#
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

/// A plugin that, when it is sent a lot of text (a message of more than 100 000 bytes),
/// counts to `iterations` before it draws: slow work that a flat budget cannot cover.
fn parses_slowly(iterations: u32) -> Vec<u8> {
    let hello = escape(HELLO);
    assemble(format!(
        r#"(module
            (import "host" "send" (func $send (param i32 i32)))
            (memory (export "memory") 40)
            (data (i32.const 0) "{hello}")
            (func (export "alloc") (param i32) (result i32) (i32.const 65536))
            (func (export "handle") (param i32 i32)
              (local $n i32)
              (if (i32.gt_u (local.get 1) (i32.const 100000))
                (then
                  (loop $count
                    (local.set $n (i32.add (local.get $n) (i32.const 1)))
                    (br_if $count (i32.lt_u (local.get $n) (i32.const {iterations}))))))
              (call $send (i32.const 0) (i32.const {len}))))"#,
        len = HELLO.len()
    ))
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
    /// Loads a plugin that may do nothing but draw.
    fn load(&self, plugin: &str, wasm: Vec<u8>) {
        self.runtime.load(plugin, wasm, []);
    }

    /// Loads a plugin that may draw and be sent the text of files, on the thread it
    /// asks for.
    fn load_on(&self, plugin: &str, wasm: Vec<u8>, thread: Thread) {
        self.runtime
            .load_on(plugin, wasm, [Capability::FilesText], thread);
    }

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

    rig.load("hello", sending(&[HELLO]));

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

    rig.load("looper", looping());

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

    rig.load("spinner", looping_at_start());

    assert!(matches!(rig.next(), Notice::Restarted { .. }));
    assert!(matches!(rig.next(), Notice::Restarted { .. }));
    assert!(matches!(rig.next(), Notice::Disabled { .. }));
}

#[test]
fn a_disabled_plugin_hears_nothing_more() {
    let rig = rig();
    rig.load("looper", looping());
    for _ in 0..3 {
        rig.next();
    }

    rig.runtime.send("looper", ToPlugin::Refresh);

    rig.nothing_more();
}

#[test]
fn a_restarted_plugin_is_fresh_and_draws_again() {
    let rig = rig();
    rig.load("flaky", fails_on_the_second_call());
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

    rig.load("greedy", asks_for_memory());

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

    rig.load("loud", sends_too_much());

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

    rig.load("garbage", b"this is not WebAssembly".to_vec());
    rig.load("headless", without_handle());

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

    rig.load(
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
    rig.load("a", sending(&[HELLO]));
    rig.next();

    rig.load("a", looping());

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

// --- git for plugins, at the runtime ---

use super::fixtures::{Echo, First};
use crate::git::{Branch, Change, Entry, Kind, Status};

fn status_with(paths: &[&str]) -> Status {
    Status {
        root: std::path::PathBuf::from("/repo"),
        branch: Branch {
            name: Some("main".into()),
            oid: Some("abc1234".into()),
            ..Branch::default()
        },
        entries: paths
            .iter()
            .map(|path| Entry {
                path: std::path::PathBuf::from(path),
                original: None,
                change: Change::Tracked {
                    staged: None,
                    unstaged: Some(Kind::Modified),
                },
            })
            .collect(),
    }
}

/// The text a plugin showed, which is the last message it heard.
fn shown(notice: &Notice) -> String {
    match notice {
        Notice::View {
            content: ViewNode::Lines(lines),
            ..
        } => lines[0].text.to_string(),
        other => panic!("not a view of lines: {other:?}"),
    }
}

const CHANGED_PREFIX: &str =
    r#"{"type":"git_changed","summary":{"generation":4,"branch":"main","commit":"abc1234","#;

#[test]
fn a_plugin_that_may_read_git_is_told_how_it_is_right_after_it_starts() {
    let rig = rig();
    rig.runtime
        .publish_git(Some((4, &status_with(&["a.rs", "b.rs"]))));

    rig.runtime
        .load("g", Echo::new(First::Echo).build(), [Capability::GitRead]);

    assert_eq!(shown(&rig.next()), r#"{"type":"start"}"#);
    let told = shown(&rig.next());
    assert!(told.starts_with(CHANGED_PREFIX), "{told}");
    assert!(told.contains(r#""changes":2"#), "{told}");
}

#[test]
fn a_plugin_that_may_not_read_git_is_not_told() {
    let rig = rig();
    rig.runtime.publish_git(Some((4, &status_with(&["a.rs"]))));

    rig.load("g", Echo::new(First::Echo).build());

    assert_eq!(shown(&rig.next()), r#"{"type":"start"}"#);
    rig.nothing_more();
}

#[test]
fn a_new_state_is_pushed_to_the_plugins_that_may_read_it() {
    let rig = rig();
    rig.runtime.load(
        "reader",
        Echo::new(First::Echo).build(),
        [Capability::GitRead],
    );
    rig.load("other", Echo::new(First::Echo).build());
    assert_eq!(shown(&rig.next()), r#"{"type":"start"}"#);
    assert_eq!(shown(&rig.next()), r#"{"type":"start"}"#);

    rig.runtime.publish_git(Some((7, &status_with(&["a.rs"]))));

    let Notice::View { plugin, content } = rig.next() else {
        panic!("a view");
    };
    assert_eq!(&*plugin, "reader");
    let ViewNode::Lines(lines) = content else {
        panic!("lines");
    };
    assert!(lines[0].text.contains(r#""generation":7"#));
    rig.nothing_more();
}

#[test]
fn the_plugins_are_told_when_the_repository_goes_away() {
    let rig = rig();
    rig.runtime.publish_git(Some((1, &status_with(&["a.rs"]))));
    rig.runtime
        .load("g", Echo::new(First::Echo).build(), [Capability::GitRead]);
    rig.next();
    rig.next();

    rig.runtime.publish_git(None);

    assert_eq!(shown(&rig.next()), r#"{"type":"git_unavailable"}"#);
}

#[test]
fn a_plugin_that_traps_is_told_how_git_is_again_when_it_starts_fresh() {
    let rig = rig();
    rig.runtime.publish_git(Some((4, &status_with(&["a.rs"]))));
    rig.runtime.load(
        "g",
        Echo::new(First::Echo).traps_on_refresh().build(),
        [Capability::GitRead],
    );
    assert_eq!(shown(&rig.next()), r#"{"type":"start"}"#);
    assert!(shown(&rig.next()).starts_with(CHANGED_PREFIX));

    rig.runtime.send("g", ToPlugin::Refresh);

    assert!(matches!(rig.next(), Notice::Restarted { .. }));
    assert_eq!(shown(&rig.next()), r#"{"type":"start"}"#);
    assert!(
        shown(&rig.next()).starts_with(CHANGED_PREFIX),
        "a fresh instance knows nothing"
    );
}

fn asking(request: &str) -> Vec<u8> {
    Echo::new(First::Echo).asks_on_action(request).build()
}

/// What the plugin showed after it was sent an action that makes it ask.
fn answer_to(rig: &Rig, plugin: &str) -> String {
    rig.runtime.send(plugin, ToPlugin::action("go"));
    // The action is echoed first, then the answer to what it asked.
    assert_eq!(shown(&rig.next()), r#"{"type":"action","action":"go"}"#);
    shown(&rig.next())
}

#[test]
fn a_page_is_answered_from_the_state_the_runtime_holds_with_its_generation() {
    let rig = rig();
    rig.runtime
        .publish_git(Some((4, &status_with(&["a.rs", "b.rs", "c.rs"]))));
    rig.runtime.load(
        "g",
        asking(r#"{"type":"git_entries","request":5,"section":"changes","offset":1,"limit":1}"#),
        [Capability::GitRead],
    );
    rig.next();
    rig.next();

    let answer = answer_to(&rig, "g");

    assert_eq!(
        answer,
        r#"{"type":"git_entries","request":5,"generation":4,"section":"changes","offset":1,"total":3,"entries":[{"path":"b.rs","original":null,"kind":"modified"}]}"#
    );
}

#[test]
fn a_page_is_always_from_the_latest_state() {
    let rig = rig();
    rig.runtime.publish_git(Some((4, &status_with(&["a.rs"]))));
    rig.runtime.load(
        "g",
        asking(r#"{"type":"git_entries","request":5,"section":"changes","limit":10}"#),
        [Capability::GitRead],
    );
    rig.next();
    rig.next();
    rig.runtime
        .publish_git(Some((5, &status_with(&["a.rs", "z.rs"]))));
    rig.next();

    let answer = answer_to(&rig, "g");

    assert!(answer.contains(r#""generation":5"#), "{answer}");
    assert!(answer.contains(r#""total":2"#), "{answer}");
}

#[test]
fn a_page_is_cut_to_the_most_entries_it_may_have_and_the_offset_may_run_out() {
    let rig = rig();
    let many: Vec<String> = (0..700).map(|n| format!("f{n:03}.rs")).collect();
    let names: Vec<&str> = many.iter().map(String::as_str).collect();
    rig.runtime.publish_git(Some((1, &status_with(&names))));
    rig.runtime.load(
        "big",
        asking(r#"{"type":"git_entries","request":1,"section":"changes","limit":100000}"#),
        [Capability::GitRead],
    );
    rig.runtime.load(
        "past",
        asking(r#"{"type":"git_entries","request":2,"section":"changes","offset":9000,"limit":5}"#),
        [Capability::GitRead],
    );
    for _ in 0..4 {
        rig.next();
    }

    let big = answer_to(&rig, "big");
    let past = answer_to(&rig, "past");

    let ToPlugin::GitEntries { entries, total, .. } = ToPlugin::from_bytes(big.as_bytes()).unwrap()
    else {
        panic!("{big}");
    };
    assert_eq!((entries.len(), total), (crate::plugin_api::MAX_PAGE, 700));
    let ToPlugin::GitEntries {
        entries, offset, ..
    } = ToPlugin::from_bytes(past.as_bytes()).unwrap()
    else {
        panic!("{past}");
    };
    assert_eq!(
        (entries.len(), offset),
        (0, 700),
        "no entries, from the end"
    );
}

#[test]
fn the_sections_are_the_ones_of_the_git_pane() {
    let rig = rig();
    let status = Status {
        root: std::path::PathBuf::from("/repo"),
        branch: Branch::default(),
        entries: vec![
            Entry {
                path: "both.rs".into(),
                original: None,
                change: Change::Tracked {
                    staged: Some(Kind::Added),
                    unstaged: Some(Kind::Modified),
                },
            },
            Entry {
                path: "moved.rs".into(),
                original: Some("old.rs".into()),
                change: Change::Tracked {
                    staged: Some(Kind::Renamed),
                    unstaged: None,
                },
            },
            Entry {
                path: "new.rs".into(),
                original: None,
                change: Change::Untracked,
            },
            Entry {
                path: "clash.rs".into(),
                original: None,
                change: Change::Conflicted,
            },
        ],
    };
    rig.runtime.publish_git(Some((1, &status)));
    let mut sections = Vec::new();
    for (index, name) in ["staged", "changes", "untracked", "conflicts"]
        .into_iter()
        .enumerate()
    {
        let request =
            format!(r#"{{"type":"git_entries","request":{index},"section":"{name}","limit":10}}"#);
        let plugin = format!("p{index}");
        rig.runtime
            .load(&plugin, asking(&request), [Capability::GitRead]);
        rig.next();
        rig.next();
        sections.push(answer_to(&rig, &plugin));
    }

    let paths = |answer: &str| {
        let Ok(ToPlugin::GitEntries { entries, .. }) = ToPlugin::from_bytes(answer.as_bytes())
        else {
            panic!("{answer}");
        };
        entries
            .into_iter()
            .map(|entry| (entry.path, entry.original))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        paths(&sections[0]),
        vec![
            ("both.rs".to_owned(), None),
            ("moved.rs".to_owned(), Some("old.rs".to_owned()))
        ]
    );
    assert_eq!(paths(&sections[1]), vec![("both.rs".to_owned(), None)]);
    assert_eq!(paths(&sections[2]), vec![("new.rs".to_owned(), None)]);
    assert_eq!(paths(&sections[3]), vec![("clash.rs".to_owned(), None)]);
}

#[test]
fn a_request_without_the_capability_is_refused_naming_it() {
    let rig = rig();
    rig.runtime.publish_git(Some((1, &status_with(&["a.rs"]))));
    rig.load(
        "g",
        asking(r#"{"type":"git_entries","request":5,"section":"changes","limit":10}"#),
    );
    rig.next();

    let answer = answer_to(&rig, "g");

    assert_eq!(
        answer,
        r#"{"type":"refused","request":5,"reason":{"type":"denied","capability":"git.read"}}"#
    );
}

#[test]
fn a_request_without_a_repository_is_refused() {
    let rig = rig();
    rig.runtime.load(
        "g",
        asking(r#"{"type":"git_entries","request":5,"section":"changes","limit":10}"#),
        [Capability::GitRead],
    );
    rig.next();

    let answer = answer_to(&rig, "g");

    assert_eq!(
        answer,
        r#"{"type":"refused","request":5,"reason":{"type":"no_repository"}}"#
    );
}

#[test]
fn a_section_this_version_does_not_have_is_a_bad_argument() {
    let rig = rig();
    rig.runtime.publish_git(Some((1, &status_with(&["a.rs"]))));
    rig.runtime.load(
        "g",
        asking(r#"{"type":"git_entries","request":5,"section":"stashes","limit":10}"#),
        [Capability::GitRead],
    );
    rig.next();
    rig.next();

    let answer = answer_to(&rig, "g");

    assert!(answer.contains(r#""type":"bad_argument""#), "{answer}");
}

#[test]
fn a_change_to_the_repository_needs_its_own_capability() {
    let rig = rig();
    rig.runtime.publish_git(Some((1, &status_with(&["a.rs"]))));
    rig.runtime.load(
        "reader",
        asking(r#"{"type":"git_stage","request":6,"path":"a.rs"}"#),
        [Capability::GitRead],
    );
    rig.next();
    rig.next();

    let answer = answer_to(&rig, "reader");

    assert_eq!(
        answer,
        r#"{"type":"refused","request":6,"reason":{"type":"denied","capability":"git.write"}}"#
    );
}

#[test]
fn a_change_with_the_capability_is_handed_to_the_app() {
    let rig = rig();
    rig.runtime.publish_git(Some((1, &status_with(&["a.rs"]))));
    rig.runtime.load(
        "writer",
        asking(r#"{"type":"git_unstage","request":6,"path":"src/a.rs"}"#),
        [Capability::GitWrite],
    );
    rig.next();
    rig.runtime.send("writer", ToPlugin::action("go"));
    rig.next();

    let notice = rig.next();

    assert_eq!(
        notice,
        Notice::GitWrite {
            plugin: "writer".into(),
            request: 6,
            op: super::WriteOp::Unstage("src/a.rs".into()),
        }
    );
}

#[test]
fn a_plugin_that_answers_every_answer_with_a_question_does_not_starve_the_others() {
    let rig = rig();
    rig.runtime.publish_git(Some((1, &status_with(&["a.rs"]))));
    rig.runtime.load(
        "chatty",
        Echo::new(First::Echo)
            .asks_always(r#"{"type":"git_entries","request":1,"section":"changes","limit":1}"#)
            .build(),
        [Capability::GitRead],
    );

    rig.load("quiet", sending(&[HELLO]));

    // The chatty plugin keeps the thread busy for ever; the quiet one still gets its turn.
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match rig.notices.recv_timeout(Duration::from_secs(5)) {
            Ok(Notice::View { plugin, .. }) if &*plugin == "quiet" => break,
            Ok(_) => assert!(
                Instant::now() < deadline,
                "the quiet plugin waited too long"
            ),
            Err(error) => panic!("{error:?}"),
        }
    }
}

// --- opening files ---

#[test]
fn a_file_to_open_is_handed_to_the_app_when_the_plugin_may() {
    let rig = rig();
    rig.runtime.load(
        "opener",
        asking(r#"{"type":"open_file","request":3,"path":"src/a.rs"}"#),
        [Capability::EditorOpen],
    );
    rig.next();
    rig.runtime.send("opener", ToPlugin::action("go"));
    rig.next();

    let notice = rig.next();

    assert_eq!(
        notice,
        Notice::OpenFile {
            plugin: "opener".into(),
            request: 3,
            path: "src/a.rs".into(),
        }
    );
}

#[test]
fn a_file_to_open_needs_its_own_capability_and_no_repository() {
    let rig = rig();
    rig.load(
        "opener",
        asking(r#"{"type":"open_file","request":3,"path":"a.rs"}"#),
    );
    rig.next();

    let answer = answer_to(&rig, "opener");

    assert_eq!(
        answer,
        r#"{"type":"refused","request":3,"reason":{"type":"denied","capability":"editor.open"}}"#
    );
}

#[test]
fn rows_with_spans_become_rows_of_spans_and_plain_lines_one_span() {
    use crate::plugin_api::WireSpan;
    let view = WireView::rows(
        [
            WireLine::new("header", "explorer.dir"),
            WireLine::of_spans([
                WireSpan::new("M ", "git.modified"),
                WireSpan::new("a.rs", ""),
            ]),
        ],
        Some(1),
    );

    let ViewNode::Rows { rows, selected } = node_of(view) else {
        panic!("rows");
    };

    assert_eq!(selected, Some(1));
    assert_eq!(rows[0].spans.len(), 1);
    assert_eq!(&*rows[0].spans[0].text, "header");
    assert_eq!(&*rows[0].spans[0].slot, "explorer.dir");
    assert_eq!(rows[1].spans.len(), 2);
    assert_eq!(&*rows[1].spans[0].slot, "git.modified");
}

// --- a thread of its own ---

fn file_text(bytes: usize) -> ToPlugin {
    ToPlugin::FileText {
        path: "a.md".into(),
        text: "x".repeat(bytes),
        modified: false,
    }
}

#[test]
fn the_budget_grows_with_the_text_only_on_a_thread_of_its_own_and_has_a_ceiling() {
    // The shared thread has the flat budget for everything.
    assert_eq!(
        fuel_for(&file_text(1_000_000), false, 1_000_000),
        CALLBACK_FUEL
    );
    // A thread of its own: the flat one and so much for each byte.
    assert_eq!(fuel_for(&file_text(0), true, 0), CALLBACK_FUEL);
    assert_eq!(
        fuel_for(&file_text(100_000), true, 0),
        CALLBACK_FUEL + 100_000 * FUEL_PER_BYTE
    );
    // Never more than the ceiling, however big.
    assert_eq!(fuel_for(&file_text(2_000_000), true, 0), OWNED_FUEL_CEILING);
    // Only what it holds of a file grows it: a message after the text gets as much as
    // the text did, and one before any text, the flat one.
    assert_eq!(
        fuel_for(&ToPlugin::Refresh, true, 100_000),
        CALLBACK_FUEL + 100_000 * FUEL_PER_BYTE
    );
    assert_eq!(fuel_for(&ToPlugin::Refresh, false, 100_000), CALLBACK_FUEL);
    assert_eq!(fuel_for(&ToPlugin::Start, true, 0), CALLBACK_FUEL);
    assert_eq!(fuel_for(&ToPlugin::Refresh, true, 0), CALLBACK_FUEL);
}

#[test]
fn a_big_text_gets_the_ceiling_and_a_text_of_the_size_of_a_readme_gets_much_less() {
    // 1 MB is far past where the rate meets the ceiling.
    assert_eq!(
        fuel_for(&file_text(1024 * 1024), true, 0),
        OWNED_FUEL_CEILING
    );
    // 20 KB: 3 times the measured cost of a dense document, well under it.
    assert!(fuel_for(&file_text(20 * 1024), true, 0) < OWNED_FUEL_CEILING / 5);
}

#[test]
fn slow_work_on_a_big_text_traps_on_the_shared_thread_and_finishes_on_its_own() {
    let rig = rig();
    // About 70 million fuel: far over the flat budget, within what 500 KB gets.
    rig.load_on("shared", parses_slowly(10_000_000), Thread::Shared);
    rig.next();
    rig.runtime.send("shared", file_text(500_000));
    assert!(matches!(rig.next(), Notice::Restarted { .. }));
    // It starts again, fresh, and draws.
    rig.next();

    rig.load_on("own", parses_slowly(10_000_000), Thread::Own);
    rig.next();
    rig.runtime.send("own", file_text(500_000));
    assert_eq!(
        rig.next(),
        Notice::View {
            plugin: "own".into(),
            content: lines("hello from a plugin", "explorer.file"),
        }
    );
}

fn reply_of(bytes: usize) -> ToPlugin {
    ToPlugin::Reply {
        from: "adapter".into(),
        request: 1,
        payload: "x".repeat(bytes),
        last: true,
    }
}

fn call_of(bytes: usize) -> ToPlugin {
    ToPlugin::Call {
        from: "pane".into(),
        request: 1,
        method: "discover".into(),
        payload: "x".repeat(bytes),
    }
}

#[test]
fn the_budget_grows_with_the_payload_of_a_call_or_a_reply_and_the_text_of_a_command_on_its_own_thread()
 {
    for message in [reply_of(100_000), call_of(100_000)] {
        // The shared thread has the flat budget whatever it is sent.
        assert_eq!(fuel_for(&message, false, 0), CALLBACK_FUEL);
        assert_eq!(
            fuel_for(&message, true, 0),
            CALLBACK_FUEL + 100_000 * FUEL_PER_BYTE
        );
    }
    let output = ToPlugin::RunOutput {
        request: 1,
        stream: crate::plugin_api::RunStream::Stdout,
        text: "x".repeat(100_000),
    };
    assert_eq!(fuel_for(&output, false, 0), CALLBACK_FUEL);
    assert_eq!(
        fuel_for(&output, true, 0),
        CALLBACK_FUEL + 100_000 * FUEL_PER_BYTE
    );
}

#[test]
fn a_small_payload_gets_the_flat_budget_and_a_huge_one_the_ceiling() {
    assert_eq!(fuel_for(&reply_of(0), true, 0), CALLBACK_FUEL);
    assert_eq!(fuel_for(&reply_of(3_000_000), true, 0), OWNED_FUEL_CEILING);
}

#[test]
fn a_payload_is_charged_as_much_as_the_text_the_plugin_holds_when_that_is_more() {
    // It holds 50 000 bytes of a file and is sent a payload of 10: not less than before.
    assert_eq!(
        fuel_for(&reply_of(10), true, 50_000),
        CALLBACK_FUEL + 50_000 * FUEL_PER_BYTE
    );
    assert_eq!(
        fuel_for(&reply_of(120_000), true, 50_000),
        CALLBACK_FUEL + 120_000 * FUEL_PER_BYTE
    );
}

#[test]
fn slow_work_on_a_big_reply_traps_on_the_shared_thread_and_finishes_on_its_own() {
    let rig = rig();
    // About 70 million fuel: far over the flat budget, within what 500 KB gets.
    rig.load_on("shared", parses_slowly(10_000_000), Thread::Shared);
    rig.next();
    rig.runtime.send("shared", reply_of(500_000));
    assert!(matches!(rig.next(), Notice::Restarted { .. }));
    rig.next();

    rig.load_on("own", parses_slowly(10_000_000), Thread::Own);
    rig.next();
    rig.runtime.send("own", reply_of(500_000));
    assert_eq!(
        rig.next(),
        Notice::View {
            plugin: "own".into(),
            content: lines("hello from a plugin", "explorer.file"),
        }
    );
}

#[test]
fn a_plugin_busy_on_its_own_thread_does_not_hold_up_the_shared_one() {
    let rig = rig();
    rig.load("quick", sending(&[HELLO]));
    rig.next();
    rig.load_on("slow", parses_slowly(10_000_000), Thread::Own);
    rig.next();

    // The slow one is sent a big text, then the quick one a message: the quick one
    // answers while the slow one is still counting.
    rig.runtime.send("slow", file_text(500_000));
    let started = Instant::now();
    rig.runtime.send("quick", ToPlugin::Refresh);
    let first = rig.next();
    assert!(
        matches!(&first, Notice::View { plugin, .. } if &**plugin == "quick"),
        "{first:?} after {:?}",
        started.elapsed()
    );
    // And the slow one does answer, after.
    let second = rig.next();
    assert!(
        matches!(&second, Notice::View { plugin, .. } if &**plugin == "slow"),
        "{second:?}"
    );
}

#[test]
fn a_plugin_that_moves_between_threads_is_only_in_one() {
    let rig = rig();
    rig.load("move", sending(&[HELLO]));
    rig.next();
    // Loaded again on a thread of its own: it starts there, and the shared one lets go.
    rig.load_on("move", sending(&[HELLO]), Thread::Own);
    rig.next();
    rig.runtime.send("move", ToPlugin::Refresh);
    rig.next();
    rig.nothing_more();

    // And back.
    rig.load("move", sending(&[HELLO]));
    rig.next();
    rig.runtime.send("move", ToPlugin::Refresh);
    rig.next();
    rig.nothing_more();

    rig.runtime.unload("move");
    rig.runtime.send("move", ToPlugin::Refresh);
    rig.nothing_more();
}

#[test]
fn a_plugin_on_its_own_thread_that_starts_later_is_told_how_git_is() {
    let rig = rig();
    rig.runtime.publish_git(None);
    // A plugin that draws what it is told: the text of the message.
    rig.runtime.load_on(
        "reader",
        crate::plugins::fixtures::Echo::new(crate::plugins::fixtures::First::Echo).build(),
        [Capability::GitRead],
        Thread::Own,
    );
    let mut seen = String::new();
    for _ in 0..2 {
        if let Notice::View {
            content: ViewNode::Lines(lines),
            ..
        } = rig.next()
        {
            seen = lines[0].text.to_string();
        }
    }
    assert!(seen.contains("git_unavailable"), "{seen}");
}
