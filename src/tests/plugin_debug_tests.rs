//! What a plugin may ask of the editor that shows or starts something (plugin API 1.11): a popup
//! of text, and a program under the debugger.

#![cfg(unix)]

use std::sync::{Arc, Mutex};

use crossterm::event::KeyCode;
use serde_json::{Value, json};

use super::debug_tests::{is_stopped, scripted};
use super::run_tests::{Rig, Setup, rig_with};
use crate::app::state::Popup;
use crate::components::debug::model::Status;
use crate::event::action::Action;
use crate::plugin_api::Capability;
use crate::plugins::fixtures::{Echo, First, TYPE_AT};

/// `refresh` is the only message with an `r` as its fourth letter (`refused`, which a plugin
/// that is told no hears, has a `u`).
const ON_REFRESH: u32 = TYPE_AT + 3;

const POPUP: &str = r#"{"type":"popup","title":"tests::fails","text":"left: 1\nright: 2\n  indented # not a heading"}"#;

fn screen(rig: &Rig) -> String {
    rig.terminal.backend().to_string()
}

/// Lets the app handle what has come to it, a moment.
fn settle(rig: &mut Rig) {
    for _ in 0..20 {
        while let Ok(event) = rig.events.try_recv() {
            rig.app.handle_event(event);
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    rig.draw();
}

fn session_goes(rig: &Rig) -> bool {
    matches!(
        rig.app.state().debug().status,
        Status::Starting | Status::Running | Status::Stopped { .. }
    )
}

/// Gets the plugin to ask, answers the question about trusting the folder with a yes, and gets it
/// to ask again, as a plugin does when it is told the folder is trusted.
fn ask_and_trust(rig: &mut Rig) {
    rig.app.refresh_plugin("tests");
    rig.wait_for("the question about the folder", |rig| {
        matches!(rig.app.state().popup(), Popup::Confirm(_))
    });
    rig.press(KeyCode::Char('y'));
    rig.app.refresh_plugin("tests");
}

/// A plugin that says `message` when the editor refreshes it, in a project that is trusted.
fn rig(message: &str, adapter: Option<crate::app::debug::Adapter>) -> Rig {
    let mut rig = rig_with(Setup::default(), adapter);
    let module = Echo::new(First::Echo)
        .asks_on_byte(ON_REFRESH, b'r', message)
        .build();
    rig.app
        .load_plugin("tests", module, Vec::new(), &[Capability::DebugStart]);
    rig
}

fn debug_message(root: &str) -> String {
    format!(
        r#"{{"type":"debug","request":5,"program":"/p/target/fx-1","args":["--exact","a::b"],"env":["K=v"],"cwd":"{root}"}}"#
    )
}

#[test]
fn a_popup_of_text_is_shown_as_it_is_and_esc_closes_it() {
    let mut rig = rig(POPUP, None);

    rig.app.refresh_plugin("tests");
    rig.wait_for("the popup", |rig| {
        matches!(rig.app.state().popup(), Popup::PluginReadme(_))
    });

    let screen = screen(&rig);
    assert!(screen.contains("tests::fails"), "{screen}");
    assert!(
        screen.contains("left: 1") && screen.contains("right: 2"),
        "{screen}"
    );
    // Nothing in it is Markdown: the line is not a heading, and it keeps its mark.
    assert!(screen.contains("indented # not a heading"), "{screen}");
    assert!(screen.contains("esc: close"), "{screen}");
    rig.press(KeyCode::Esc);
    assert!(matches!(rig.app.state().popup(), Popup::None));
}

#[test]
fn a_popup_is_not_opened_over_another_popup() {
    let mut rig = rig(POPUP, None);
    rig.app.run_action(Action::CommandPalette);
    assert!(!matches!(rig.app.state().popup(), Popup::None));

    rig.app.refresh_plugin("tests");
    settle(&mut rig);

    assert!(!matches!(rig.app.state().popup(), Popup::PluginReadme(_)));
}

#[test]
fn a_program_a_plugin_asks_to_debug_is_launched_with_its_arguments_in_its_folder() {
    let launched: Arc<Mutex<Option<Value>>> = Arc::default();
    let seen = Arc::clone(&launched);
    let adapter = scripted(move |adapter| {
        adapter.answer_initialize(json!({ "supportsConfigurationDoneRequest": true }));
        let (_, launching) = adapter.expect_request("launch");
        *seen.lock().unwrap() = Some(launching);
        while adapter.next().is_some() {}
    });
    let mut rig = rig(&String::new(), Some(adapter));
    let message = debug_message("");
    let module = Echo::new(First::Echo)
        .asks_on_byte(ON_REFRESH, b'r', &message)
        .build();
    rig.app
        .load_plugin("tests", module, Vec::new(), &[Capability::DebugStart]);

    ask_and_trust(&mut rig);

    rig.wait_for("the launch", |_| launched.lock().unwrap().is_some());
    let launching = launched.lock().unwrap().clone().unwrap();
    assert_eq!(launching["program"], "/p/target/fx-1");
    assert_eq!(launching["args"], json!(["--exact", "a::b"]));
    assert!(
        launching["env"].as_array().unwrap().contains(&json!("K=v")),
        "{launching}"
    );
    assert_eq!(launching["cwd"], rig.root.display().to_string());
    assert_eq!(launching["runInTerminal"], true);
    assert!(rig.app.state().terminal().tab_count() >= 1);
}

#[test]
fn a_session_a_plugin_started_is_not_restarted_as_a_run_of_the_project() {
    let adapter = scripted(|adapter| {
        adapter.answer_initialize(json!({ "supportsConfigurationDoneRequest": true }));
        let _ = adapter.expect_request("launch");
        while adapter.next().is_some() {}
    });
    let message = debug_message("");
    let mut rig = rig(&message, Some(adapter));

    ask_and_trust(&mut rig);
    rig.wait_for("the session", |rig| session_goes(rig));
    rig.app.run_action(Action::DebugRestart);
    settle(&mut rig);

    // Still the same session, and the user was told where to restart it.
    assert!(session_goes(&rig));
    assert!(!is_stopped(&rig.app.state().debug().status));
    assert!(rig.said_has("started by a plugin"), "{:?}", rig.said());
}
