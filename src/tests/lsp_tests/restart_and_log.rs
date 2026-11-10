//! "LSP: Restart Server of This File", "LSP: Restart All Servers", "LSP: Show Server Log of This
//! File" and "Editor: Restart".

use super::*;
use crate::event::action::Action;

/// A rig whose server logs a line when it is told about a document and passes on what it is sent.
fn running() -> (Rig, Receiver<Message>) {
    let (dir, file) = rust_project();
    write(&file, "fn main() {}\n");
    let (sent, received) = std::sync::mpsc::channel();
    let launcher = FakeLauncher::new(move |_| {
        let sent = sent.clone();
        Some(Box::new(move |session| {
            while let Some(message) = session.next() {
                if method_of(&message) == Some("textDocument/didOpen") {
                    session.notify(
                        "window/logMessage",
                        json!({ "type": 3, "message": "loaded the workspace" }),
                    );
                }
                let _ = sent.send(message);
            }
        }))
    });
    let mut rig = rig(dir.path(), std::slice::from_ref(&file), launcher);
    std::mem::forget(dir);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();
    (rig, received)
}

impl Rig {
    /// Takes what the servers sent to the app for `time`.
    fn pump_for(&mut self, time: Duration) {
        let end = Instant::now() + time;
        while Instant::now() < end {
            while let Ok(event) = self.events.try_recv() {
                self.app.handle_event(event);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn did_opens(received: &Receiver<Message>) -> usize {
    let mut count = 0;
    while let Ok(message) = received.recv_timeout(Duration::from_millis(200)) {
        if method_of(&message) == Some("textDocument/didOpen") {
            count += 1;
        }
    }
    count
}

#[test]
fn restarting_the_server_starts_it_again_and_tells_it_about_the_open_file_again() {
    let (mut rig, received) = running();
    assert_eq!(did_opens(&received), 1);
    assert_eq!(rig.launcher.launched(), 1);

    rig.app.run_action(Action::RestartLspServer);
    rig.wait_until("the second start", |rig| rig.launcher.launched() == 2);
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();

    assert!(
        rig.texts().iter().any(|text| text == "restarting rust"),
        "{:?}",
        rig.texts()
    );
    assert_eq!(
        did_opens(&received),
        1,
        "the new server was told about the file"
    );
}

#[test]
fn restart_all_ends_every_server_and_the_one_of_the_file_in_front_comes_back() {
    let (mut rig, _received) = running();

    rig.app.run_action(Action::RestartLspServers);
    rig.wait_until("the second start", |rig| rig.launcher.launched() == 2);
    rig.wait_for_status(Some("rust-analyzer"));

    assert!(
        rig.texts()
            .iter()
            .any(|text| text.contains("restarting the language servers"))
    );
}

#[test]
fn restart_says_so_when_the_file_has_no_server() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    write(&file, "text");
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| stays()));

    rig.app.run_action(Action::RestartLspServer);

    assert!(
        rig.texts()
            .iter()
            .any(|text| text.contains("has no language server to restart")),
        "{:?}",
        rig.texts()
    );
    assert_eq!(rig.launcher.launched(), 0);
}

#[test]
fn restart_before_the_server_was_ever_asked_for_says_so() {
    let (dir, file) = rust_project();
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| stays()));

    rig.app.run_action(Action::RestartLspServer);

    assert!(
        rig.texts()
            .iter()
            .any(|text| text.contains("has not been started yet")),
        "{:?}",
        rig.texts()
    );
}

#[test]
fn the_log_opens_in_a_tab_that_cannot_be_edited_with_what_the_server_said() {
    let (mut rig, _received) = running();
    rig.pump_for(Duration::from_millis(400));
    rig.app.run_action(Action::ShowLspLog);
    assert_eq!(rig.app.state().editor().buffer().title(), Some("rust log"));

    let buffer = rig.app.state().editor().buffer();
    let text = buffer.text();
    assert!(buffer.is_read_only());
    assert!(text.starts_with("# rust: rust-analyzer"), "{text}");
    assert!(text.contains("-- started"), "{text}");
    assert!(text.contains("log (info): loaded the workspace"), "{text}");
}

#[test]
fn the_log_shows_a_restart_and_keeps_what_came_before_it() {
    let (mut rig, _received) = running();
    rig.app.run_action(Action::RestartLspServer);
    rig.wait_until("the second start", |rig| rig.launcher.launched() == 2);
    rig.wait_for_status(Some("rust-analyzer"));
    rig.pump_for(Duration::from_millis(200));

    rig.app.run_action(Action::ShowLspLog);

    let text = rig.app.state().editor().buffer().text();
    let started = text.find("-- started").expect("started");
    let restarted = text
        .find("-- restarted at the user's request")
        .expect("restarted");
    assert!(started < restarted, "{text}");
    assert_eq!(text.matches("-- started").count(), 2, "{text}");
}

#[test]
fn the_log_of_a_server_that_has_said_nothing_says_so_instead_of_opening_an_empty_tab() {
    let (dir, file) = rust_project();
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| stays()));
    let tabs = rig.app.state().workspace().tab_count_for_test();

    rig.app.run_action(Action::ShowLspLog);

    assert_eq!(rig.app.state().workspace().tab_count_for_test(), tabs);
    assert!(
        rig.texts()
            .iter()
            .any(|text| text.contains("has said nothing yet")),
        "{:?}",
        rig.texts()
    );
}

#[test]
fn the_log_of_a_file_with_no_server_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    write(&file, "text");
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| stays()));

    rig.app.run_action(Action::ShowLspLog);

    assert!(
        rig.texts()
            .iter()
            .any(|text| text.contains("no language server, so there is no log"))
    );
}

#[test]
fn restart_editor_ends_the_editor_to_start_again_in_the_project_folder() {
    let (dir, file) = rust_project();
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| stays()));

    rig.app.run_action(Action::RestartEditor);

    assert!(rig.app.should_quit());
    let target = rig
        .app
        .restart_target()
        .expect("a folder to start again in");
    assert_eq!(target, crate::files::paths::canonical(dir.path()).as_path());
}

#[test]
fn restart_editor_asks_about_unsaved_files_first() {
    let (dir, file) = rust_project();
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| stays()));
    rig.app
        .state_mut()
        .edit(|editor| editor.insert_text("x").unwrap());

    rig.app.run_action(Action::RestartEditor);

    assert!(!rig.app.should_quit(), "the quit prompt asks first");
}

#[test]
fn the_four_are_in_the_palette() {
    for action in [
        Action::RestartLspServer,
        Action::RestartLspServers,
        Action::ShowLspLog,
        Action::RestartEditor,
    ] {
        assert!(Action::palette_actions().contains(&action), "{action:?}");
    }
}
