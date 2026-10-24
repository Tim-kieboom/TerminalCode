//! The editor debugging a real project with the real `lldb-dap`: ignored unless asked for.
//!
//! ```text
//! TRCODE_DEBUG_ADAPTER=/path/to/lldb-dap cargo test -p terminal_code real_debug -- --ignored
//! ```
//!
//! They need `cargo` too, and `lldb-dap` (LLVM) on the `PATH` or named by the variable.

#![cfg(unix)]

use std::fs;

use tokio::sync::mpsc;

use super::run_tests::Rig;
use crate::app::App;
use crate::app::state::AppState;
use crate::components::debug::model::Status;
use crate::event::action::Action;
use crate::persist::saved_state::SavedState;

const MAIN: &str = "fn main() {\n    let mut total = 0;\n    for i in 1..=3 {\n        total += i;\n    }\n    println!(\"total = {total}\");\n}\n";
/// The line of `total += i`, from 0.
const BREAKPOINT: usize = 3;

fn project() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"debug-demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(root.join("src/main.rs"), MAIN).unwrap();
    let config = tempfile::tempdir().unwrap();
    let mut state = AppState::default();
    state.open_project(&root).unwrap();
    let (tx, events) = mpsc::channel(1024);
    let app = App::new(state)
        .with_events(tx)
        .with_config_dir(Some(config.path().to_path_buf()))
        .with_saved_state(SavedState::default());
    let mut rig = Rig {
        app,
        events,
        terminal: ratatui::Terminal::new(ratatui::backend::TestBackend::new(200, 30)).unwrap(),
        _dir: dir,
        config,
        root,
    };
    rig.draw();
    rig
}

#[test]
#[ignore = "needs cargo and lldb-dap: TRCODE_DEBUG_ADAPTER=/path/to/lldb-dap"]
fn the_editor_debugs_a_real_cargo_project_with_the_real_lldb_dap() {
    let mut rig = project();
    rig.app
        .state_mut()
        .workspace_mut()
        .open_path(&rig.root.join("src/main.rs"))
        .unwrap();
    rig.app.handle_event(crate::event::Event::TerminalChanged);
    rig.breakpoint_on(BREAKPOINT);

    rig.app.run_action(Action::Debug);
    rig.wait_for_status("the first stop", |status| {
        matches!(status, Status::Stopped { .. })
    });

    // Where it stopped, the stack and the variables of the chosen frame.
    rig.wait_for("the stack and the variables", |rig| {
        let data = rig.app.state().debug();
        !data.frames.is_empty() && data.tree.rows().iter().any(|row| row.name == "total")
    });
    let data = rig.app.state().debug();
    let top = &data.frames[0];
    assert!(top.name.contains("main"), "{top:?}");
    assert_eq!(top.line, BREAKPOINT, "{top:?}");
    assert!(
        top.path.as_ref().unwrap().ends_with("src/main.rs"),
        "{top:?}"
    );
    let names: Vec<&str> = data
        .tree
        .rows()
        .iter()
        .map(|row| row.name.as_str())
        .collect();
    assert!(names.contains(&"total"), "{names:?}");
    // The editor shows the line, and the arrow is in the gutter.
    assert_eq!(
        rig.app.state().editor().selections().primary().head().line,
        BREAKPOINT
    );
    rig.draw();
    assert!(
        rig.gutter_of("        total += i;").contains('▶'),
        "{}",
        rig.screen()
    );

    // A restart ends the session and stops at the same breakpoint again.
    rig.app.run_action(Action::DebugRestart);
    rig.wait_for_status("the first session to end", |status| {
        !matches!(status, Status::Stopped { .. })
    });
    rig.wait_for("the session to be started again", |rig| {
        let data = rig.app.state().debug();
        matches!(data.status, Status::Stopped { .. })
            && data.tree.rows().iter().any(|row| row.name == "total")
            && data
                .frames
                .first()
                .is_some_and(|frame| frame.line == BREAKPOINT)
    });

    // The breakpoint is in a loop of three: step, then go on until the program ends.
    rig.app.run_action(Action::DebugStepOver);
    rig.wait_for_status(
        "a step",
        |status| matches!(status, Status::Stopped { reason, .. } if reason == "step"),
    );
    for _ in 0..10 {
        if matches!(rig.status(), Status::Ended(_)) {
            break;
        }
        if matches!(rig.status(), Status::Stopped { .. }) {
            rig.app.run_action(Action::DebugContinue);
        }
        rig.wait_for("the next stop or the end", |rig| {
            matches!(
                rig.app.state().debug().status,
                Status::Stopped { .. } | Status::Ended(_)
            )
        });
    }
    rig.wait_for_status("the end", |status| matches!(status, Status::Ended(_)));
    rig.wait_for("the program's output", |rig| {
        rig.tab_text().contains("total = 6")
    });
}
