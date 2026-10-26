//! A plugin's popup of text, and a program it asks to debug: what reaches the app, and what
//! the plugin is told when it may not.

use std::time::{Duration, Instant};

use super::calls::{act, heard};
use super::{Rig, rig};
use crate::plugin_api::{Capability, MAX_POPUP_TEXT};
use crate::plugins::fixtures::{Echo, First};
use crate::plugins::runtime::cut_popup_text;
use crate::plugins::{Notice, Thread};

const POPUP: &str = r#"{"type":"popup","title":"tests::fails","text":"left: 1\nright: 2"}"#;
const DEBUG: &str = r#"{"type":"debug","request":5,"program":"/p/fx-1","args":["--exact","a::b"],"env":["K=v","broken"],"cwd":"sub"}"#;
const DEBUG_OUT: &str = r#"{"type":"debug","request":5,"program":"/p/fx-1","cwd":"../elsewhere"}"#;

const GOT_UNTRUSTED: &str = r#"{"type":"refused","request":5,"reason":{"type":"untrusted"}}"#;
const GOT_DENIED: &str =
    r#"{"type":"refused","request":5,"reason":{"type":"denied","capability":"debug.start"}}"#;

fn load(rig: &Rig, message: &str, grants: &[Capability]) {
    let echo = Echo::new(First::Echo).asks_on_action_named(b'd', message);
    rig.runtime.load_with(
        "tests",
        echo.build(),
        grants.iter().copied(),
        Thread::Shared,
        Vec::new(),
    );
    heard(rig, "tests", "");
}

/// The next notice that `want` takes; the others are passed over.
fn next<T>(rig: &Rig, want: impl Fn(Notice) -> Option<T>) -> T {
    let until = Instant::now() + Duration::from_secs(10);
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        let Ok(notice) = rig.notices.recv_timeout(left) else {
            break;
        };
        if let Some(found) = want(notice) {
            return found;
        }
    }
    panic!("the notice never came");
}

#[test]
fn a_popup_needs_no_capability_and_its_text_reaches_the_app() {
    let rig = rig();
    load(&rig, POPUP, &[]);

    rig.runtime
        .send("tests", crate::plugin_api::ToPlugin::action("d"));

    let (plugin, title, text) = next(&rig, |notice| match notice {
        Notice::Popup {
            plugin,
            title,
            text,
        } => Some((plugin, title, text)),
        _ => None,
    });
    assert_eq!(&*plugin, "tests");
    assert_eq!(title, "tests::fails");
    assert_eq!(text, "left: 1\nright: 2");
}

#[test]
fn text_over_the_limit_is_cut_at_a_line_and_says_so() {
    let line = "x".repeat(99) + "\n";
    let text = line.repeat(MAX_POPUP_TEXT / line.len() + 5);

    let cut = cut_popup_text(text);

    assert!(cut.len() < MAX_POPUP_TEXT + 100);
    assert!(cut.ends_with("\n[cut: the message is longer]"), "{cut:?}");
    assert!(
        cut.lines().rev().nth(1).unwrap().len() == 99,
        "the last line is whole"
    );
}

#[test]
fn text_that_fits_is_left_alone_and_a_character_is_not_cut_in_two() {
    assert_eq!(cut_popup_text("short".to_owned()), "short");
    let text = "é".repeat(MAX_POPUP_TEXT);

    let cut = cut_popup_text(text);

    assert!(cut.starts_with('é') && cut.ends_with("longer]"));
}

#[test]
fn a_program_to_debug_reaches_the_app_with_a_folder_that_is_whole() {
    let rig = rig();
    let folder = tempfile::tempdir().unwrap();
    std::fs::create_dir(folder.path().join("sub")).unwrap();
    rig.runtime.set_workspace(folder.path().to_path_buf(), true);
    load(&rig, DEBUG, &[Capability::DebugStart]);

    act(&rig, "tests", "d");

    let found = next(&rig, |notice| match notice {
        Notice::Debug {
            request,
            program,
            args,
            env,
            cwd,
            ..
        } => Some((request, program, args, env, cwd)),
        _ => None,
    });
    assert_eq!(found.0, 5);
    assert_eq!(found.1, "/p/fx-1");
    assert_eq!(found.2, ["--exact", "a::b"]);
    assert_eq!(found.3, ["K=v", "broken"]);
    assert_eq!(found.4, folder.path().join("sub").canonicalize().unwrap());
}

#[test]
fn a_program_to_debug_is_refused_without_the_capability() {
    let rig = rig();
    let folder = tempfile::tempdir().unwrap();
    rig.runtime.set_workspace(folder.path().to_path_buf(), true);
    load(&rig, DEBUG, &[]);

    act(&rig, "tests", "d");

    heard(&rig, "tests", GOT_DENIED);
}

#[test]
fn a_folder_that_is_not_trusted_refuses_the_debugger_and_asks_the_user() {
    let rig = rig();
    let folder = tempfile::tempdir().unwrap();
    rig.runtime
        .set_workspace(folder.path().to_path_buf(), false);
    load(&rig, DEBUG, &[Capability::DebugStart]);

    act(&rig, "tests", "d");

    next(&rig, |notice| {
        matches!(notice, Notice::NeedsTrust { .. }).then_some(())
    });
    heard(&rig, "tests", GOT_UNTRUSTED);
}

#[test]
fn a_folder_outside_the_project_is_refused() {
    let rig = rig();
    let folder = tempfile::tempdir().unwrap();
    rig.runtime.set_workspace(folder.path().to_path_buf(), true);
    load(&rig, DEBUG_OUT, &[Capability::DebugStart]);

    act(&rig, "tests", "d");

    heard(
        &rig,
        "tests",
        r#"{"type":"refused","request":5,"reason":{"type":"bad_argument","detail":"`../elsewhere` is not a folder inside the project"}}"#,
    );
}

#[test]
fn a_whole_path_of_a_folder_inside_the_project_is_the_folder_and_one_outside_is_refused() {
    let rig = rig();
    let folder = tempfile::tempdir().unwrap();
    std::fs::create_dir(folder.path().join("sub")).unwrap();
    rig.runtime.set_workspace(folder.path().to_path_buf(), true);
    let inside = folder.path().canonicalize().unwrap().join("sub");
    // Built as JSON: a Windows path has backslashes, which text pasted in would leave unescaped.
    let inside = serde_json::json!({
        "type": "debug", "request": 5, "program": "/p/fx-1", "cwd": inside.display().to_string(),
    })
    .to_string();
    load(&rig, &inside, &[Capability::DebugStart]);

    act(&rig, "tests", "d");

    let cwd = next(&rig, |notice| match notice {
        Notice::Debug { cwd, .. } => Some(cwd),
        _ => None,
    });
    assert_eq!(cwd, folder.path().join("sub").canonicalize().unwrap());
}

#[test]
fn a_whole_path_outside_the_project_is_refused_as_not_a_folder_of_it() {
    let rig = rig();
    let folder = tempfile::tempdir().unwrap();
    rig.runtime.set_workspace(folder.path().to_path_buf(), true);
    load(
        &rig,
        r#"{"type":"debug","request":5,"program":"/p/fx-1","cwd":"/usr"}"#,
        &[Capability::DebugStart],
    );

    act(&rig, "tests", "d");

    heard(
        &rig,
        "tests",
        r#"{"type":"refused","request":5,"reason":{"type":"bad_argument","detail":"`/usr` is not a folder inside the project"}}"#,
    );
}
