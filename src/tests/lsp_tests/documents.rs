use super::*;

#[test]
fn a_file_the_server_serves_is_opened_with_its_text_once_the_server_runs() {
    let (dir, file) = rust_project();
    let (mut rig, received) = Rig::recording(&file, dir.path());

    rig.sync();

    let opened = next_notification(&received, "textDocument/didOpen");
    let document = &opened["textDocument"];
    assert_eq!(document["languageId"], json!("rust"));
    assert_eq!(document["text"], json!("fn main() {}\n"));
    assert!(
        document["uri"].as_str().unwrap().ends_with("/src/main.rs"),
        "{document}"
    );
    assert_eq!(document["version"], json!(0));
    rig.sync();
    assert!(gets_nothing(&received), "opened once");
}

#[test]
fn nothing_is_sent_while_the_server_is_still_starting() {
    let (dir, file) = rust_project();
    let (sent, received) = std::sync::mpsc::channel::<Message>();
    let launcher = FakeLauncher::new(move |_| recording(sent.clone()));
    let mut rig = rig(dir.path(), &[file], launcher);

    rig.look();
    rig.sync();

    assert_eq!(rig.status().as_deref(), Some("rust-analyzer: starting"));
    assert!(gets_nothing(&received));
}

#[test]
fn typing_sends_the_change_with_its_position_and_the_new_version() {
    let (dir, file) = rust_project();
    let (mut rig, received) = Rig::recording(&file, dir.path());
    rig.sync();
    next_notification(&received, "textDocument/didOpen");

    // The cursor is at the start; "// " goes in front of the first line.
    rig.type_text("// ");
    rig.sync();

    let changed = next_notification(&received, "textDocument/didChange");
    assert_eq!(
        changed["contentChanges"][0]["range"],
        json!({ "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } })
    );
    assert_eq!(changed["contentChanges"][0]["text"], json!("// "));
    assert!(changed["textDocument"]["version"].as_u64().unwrap() > 0);
}

#[test]
fn typing_between_two_frames_is_one_message() {
    let (dir, file) = rust_project();
    let (mut rig, received) = Rig::recording(&file, dir.path());
    rig.sync();
    next_notification(&received, "textDocument/didOpen");

    rig.type_text("abcdef");
    rig.sync();

    let changed = next_notification(&received, "textDocument/didChange");
    assert_eq!(changed["contentChanges"][0]["text"], json!("abcdef"));
    assert!(gets_nothing(&received), "one message for six keys");
}

#[test]
fn undo_is_a_change_too() {
    let (dir, file) = rust_project();
    let (mut rig, received) = Rig::recording(&file, dir.path());
    rig.sync();
    next_notification(&received, "textDocument/didOpen");
    rig.type_text("abc");
    rig.sync();
    next_notification(&received, "textDocument/didChange");

    rig.press(KeyCode::Char('z'), KeyModifiers::CONTROL);
    rig.sync();

    let changed = next_notification(&received, "textDocument/didChange");
    assert_eq!(changed["contentChanges"][0]["text"], json!(""));
    assert_eq!(
        changed["contentChanges"][0]["range"]["end"],
        json!({ "line": 0, "character": 3 })
    );
}

#[test]
fn saving_sends_the_change_and_then_did_save() {
    let (dir, file) = rust_project();
    let (mut rig, received) = Rig::recording(&file, dir.path());
    rig.sync();
    next_notification(&received, "textDocument/didOpen");

    rig.type_text("x");
    rig.press(KeyCode::Char('s'), KeyModifiers::CONTROL);
    rig.sync();

    next_notification(&received, "textDocument/didChange");
    let saved = next_notification(&received, "textDocument/didSave");
    assert!(
        saved["textDocument"]["uri"]
            .as_str()
            .unwrap()
            .ends_with("/src/main.rs"),
        "{saved}"
    );
    rig.sync();
    assert!(gets_nothing(&received), "told once");
}

#[test]
fn closing_the_tab_closes_the_document() {
    let (dir, file) = rust_project();
    let (mut rig, received) = Rig::recording(&file, dir.path());
    rig.sync();
    next_notification(&received, "textDocument/didOpen");

    rig.press(KeyCode::Char('w'), KeyModifiers::CONTROL);
    rig.sync();

    let closed = next_notification(&received, "textDocument/didClose");
    assert!(
        closed["textDocument"]["uri"]
            .as_str()
            .unwrap()
            .ends_with("/src/main.rs"),
        "{closed}"
    );
}

#[test]
fn a_second_file_is_opened_when_it_is_shown_and_a_file_nobody_serves_is_not() {
    let (dir, file) = rust_project();
    let other = dir.path().join("src/other.rs");
    write(&other, "pub fn other() {}\n");
    let notes = dir.path().join("notes.txt");
    write(&notes, "hello\n");
    let (sent, received) = std::sync::mpsc::channel();
    let launcher = FakeLauncher::new(move |_| recording(sent.clone()));
    let mut rig = rig(dir.path(), &[file, other, notes], launcher);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));

    rig.sync();

    let first = next_notification(&received, "textDocument/didOpen");
    let second = next_notification(&received, "textDocument/didOpen");
    let mut uris = vec![
        first["textDocument"]["uri"].as_str().unwrap().to_owned(),
        second["textDocument"]["uri"].as_str().unwrap().to_owned(),
    ];
    uris.sort();
    assert!(
        uris[0].ends_with("/src/main.rs") && uris[1].ends_with("/src/other.rs"),
        "{uris:?}"
    );
    assert!(gets_nothing(&received), "the text file is not sent");
}

#[test]
fn a_server_that_started_again_is_told_about_the_open_documents_again() {
    let (dir, file) = rust_project();
    let (sent, received) = std::sync::mpsc::channel();
    // The first run dies after it has been told about the document.
    let launcher = FakeLauncher::new(move |number| {
        if number == 0 {
            recording_then_dying(sent.clone(), 1)
        } else {
            recording(sent.clone())
        }
    });
    let mut rig = rig(dir.path(), &[file], launcher);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();
    next_notification(&received, "textDocument/didOpen");

    // Wait for the restart, then sync as the loop would.
    let deadline = Instant::now() + Duration::from_secs(10);
    while rig.launcher.launched() < 2 || rig.status().as_deref() != Some("rust-analyzer") {
        while let Ok(event) = rig.events.try_recv() {
            rig.app.handle_event(event);
        }
        assert!(Instant::now() < deadline, "no restart");
        std::thread::sleep(Duration::from_millis(2));
    }
    rig.sync();

    let again = next_notification(&received, "textDocument/didOpen");
    assert_eq!(again["textDocument"]["text"], json!("fn main() {}\n"));
}

#[test]
fn a_reload_of_the_file_from_disk_reaches_the_server_as_a_change() {
    let (dir, file) = rust_project();
    let (mut rig, received) = Rig::recording(&file, dir.path());
    rig.sync();
    next_notification(&received, "textDocument/didOpen");

    fs::write(&file, "fn main() { println!(); }\n").unwrap();
    rig.app
        .handle_event(Event::FilesChanged(vec![file.clone()]));
    rig.sync();

    let changed = next_notification(&received, "textDocument/didChange");
    assert_eq!(changed["contentChanges"][0]["text"], json!(" println!(); "));
}

#[test]
fn changing_the_servers_in_the_settings_opens_the_documents_on_the_new_server() {
    let (dir, file) = rust_project();
    let config = dir.path().join("config");
    let (sent, received) = std::sync::mpsc::channel();
    let launcher = FakeLauncher::new(move |_| recording(sent.clone()));
    let mut rig = config_app(dir.path(), &file, launcher);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();
    next_notification(&received, "textDocument/didOpen");

    reload_with(
        &mut rig,
        &config,
        r#"{ "language_servers": { "rust": { "command": "ra-multiplex", "extensions": ["rs"] } } }"#,
    );
    rig.wait_for_status(Some("ra-multiplex"));
    rig.sync();

    let opened = next_notification(&received, "textDocument/didOpen");
    assert_eq!(opened["textDocument"]["text"], json!("fn main() {}\n"));
}
