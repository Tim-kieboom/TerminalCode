use super::*;

#[test]
fn what_the_server_publishes_shows_in_the_document_with_its_bytes_severity_and_message() {
    let (rig, _received, _dir, _file) = Rig::with_published(
        "fn main() {}\nlet x = 1;\n",
        vec![
            diagnostic_json((0, 3), (0, 7), 1, "the name main is taken"),
            diagnostic_json((1, 4), (1, 5), 2, "unused variable: `x`"),
        ],
    );

    assert_eq!(
        rig.diagnostics(),
        vec![
            (3..7, Severity::Error, "the name main is taken".to_owned()),
            (17..18, Severity::Warning, "unused variable: `x`".to_owned()),
        ]
    );
}

#[test]
fn positions_after_a_wide_character_are_placed_by_utf_16_units() {
    let (rig, _received, _dir, _file) = Rig::with_published(
        "a😀b\n",
        vec![diagnostic_json((0, 3), (0, 4), 1, "on the b")],
    );

    // a is 1 byte and the emoji is 4, so the b is the sixth byte.
    assert_eq!(rig.diagnostics()[0].0, 5..6);
}

#[test]
fn text_typed_before_a_diagnostic_moves_it_along() {
    let (mut rig, _received, _dir, _file) = Rig::with_published(
        "fn main() {}\n",
        vec![diagnostic_json((0, 3), (0, 7), 1, "on main")],
    );

    rig.type_text("// ");
    rig.sync();

    assert_eq!(rig.diagnostics()[0].0, 6..10);
}

#[test]
fn text_typed_inside_a_diagnostic_makes_it_longer() {
    let (mut rig, _received, _dir, _file) = Rig::with_published(
        "fn main() {}\n",
        vec![diagnostic_json((0, 3), (0, 7), 1, "on main")],
    );
    for _ in 0..5 {
        rig.press(KeyCode::Right, KeyModifiers::NONE);
    }

    rig.type_text("XY");
    rig.sync();

    assert_eq!(rig.diagnostics()[0].0, 3..9);
}

#[test]
fn deleting_text_inside_a_diagnostic_drops_it_and_the_others_stay_in_place() {
    let (mut rig, _received, _dir, _file) = Rig::with_published(
        "fn main() {}\nlet x = 1;\n",
        vec![
            diagnostic_json((0, 3), (0, 7), 1, "on main"),
            diagnostic_json((1, 4), (1, 5), 2, "on x"),
        ],
    );
    for _ in 0..5 {
        rig.press(KeyCode::Right, KeyModifiers::NONE);
    }

    rig.press(KeyCode::Backspace, KeyModifiers::NONE);
    rig.sync();

    let left = rig.diagnostics();
    assert_eq!(left.len(), 1, "{left:?}");
    assert_eq!(left[0].2, "on x");
    assert_eq!(left[0].0, 16..17, "one byte shorter before it");
}

#[test]
fn an_answer_about_an_older_version_is_placed_in_the_text_as_it_is_now() {
    let (dir, file) = rust_project();
    let (sent, _received) = std::sync::mpsc::channel();
    // A slow server: it answers the document it was opened with only after
    // it has been told about a change.
    let launcher = FakeLauncher::new(move |_| {
        responding(sent.clone(), |message| match method_of(message) {
            Some("textDocument/didChange") => {
                let uri = params_of(message)["textDocument"]["uri"].clone();
                // Version 0 is what it was opened with: "main" at columns 3 to 7.
                vec![publication(
                    &uri,
                    json!(0),
                    vec![diagnostic_json((0, 3), (0, 7), 1, "on main")],
                )]
            }
            _ => Vec::new(),
        })
    });
    let mut rig = rig(dir.path(), &[file], launcher);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();

    rig.type_text("// ");
    rig.sync();
    rig.wait_until("the late answer", |rig| !rig.diagnostics().is_empty());

    assert_eq!(
        rig.diagnostics()[0].0,
        6..10,
        "on main, which moved three bytes"
    );
}

#[test]
fn text_typed_after_an_answer_but_before_the_next_sync_is_followed_too() {
    let (mut rig, _received, _dir, _file) = Rig::with_published(
        "fn main() {}\n",
        vec![diagnostic_json((0, 3), (0, 7), 1, "on main")],
    );

    // Typed, and the frame has not happened yet: the server was not told.
    rig.type_text("// ");
    assert_eq!(
        rig.diagnostics()[0].0,
        3..7,
        "still where the server put it"
    );
    rig.sync();

    assert_eq!(rig.diagnostics()[0].0, 6..10);
}

#[test]
fn an_answer_about_a_version_that_is_not_kept_clears_the_document() {
    let (dir, file) = rust_project();
    let (sent, _received) = std::sync::mpsc::channel();
    let launcher = FakeLauncher::new(move |_| {
        responding(sent.clone(), |message| match method_of(message) {
            Some("textDocument/didOpen") => {
                let uri = params_of(message)["textDocument"]["uri"].clone();
                vec![publication(
                    &uri,
                    json!(0),
                    vec![diagnostic_json((0, 3), (0, 7), 1, "first")],
                )]
            }
            Some("textDocument/didChange") => {
                let uri = params_of(message)["textDocument"]["uri"].clone();
                vec![publication(
                    &uri,
                    json!(9999),
                    vec![diagnostic_json((0, 0), (0, 1), 1, "unplaceable")],
                )]
            }
            _ => Vec::new(),
        })
    });
    let mut rig = rig(dir.path(), &[file], launcher);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();
    rig.wait_until("the first diagnostics", |rig| !rig.diagnostics().is_empty());

    rig.type_text("x");
    rig.sync();
    rig.wait_until("the unplaceable answer to be taken", |rig| {
        rig.diagnostics().is_empty()
    });
}

#[test]
fn publishing_nothing_clears_what_there_was() {
    let (dir, file) = rust_project();
    let (sent, _received) = std::sync::mpsc::channel();
    let launcher = FakeLauncher::new(move |_| {
        responding(sent.clone(), |message| {
            let uri = params_of(message)["textDocument"]["uri"].clone();
            match method_of(message) {
                Some("textDocument/didOpen") => {
                    vec![publication(
                        &uri,
                        json!(0),
                        vec![diagnostic_json((0, 3), (0, 7), 1, "x")],
                    )]
                }
                Some("textDocument/didChange") => vec![publication(&uri, Value::Null, Vec::new())],
                _ => Vec::new(),
            }
        })
    });
    let mut rig = rig(dir.path(), &[file], launcher);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();
    rig.wait_until("diagnostics", |rig| !rig.diagnostics().is_empty());

    rig.type_text("x");
    rig.sync();

    rig.wait_until("them to be cleared", |rig| rig.diagnostics().is_empty());
}

#[test]
fn diagnostics_about_a_document_that_is_not_open_are_ignored() {
    let (dir, file) = rust_project();
    let (sent, _received) = std::sync::mpsc::channel();
    let launcher = FakeLauncher::new(move |_| {
        responding(sent.clone(), |message| match method_of(message) {
            Some("textDocument/didOpen") => vec![publication(
                &json!("file:///somewhere/else.rs"),
                json!(0),
                vec![diagnostic_json((0, 0), (0, 1), 1, "elsewhere")],
            )],
            _ => Vec::new(),
        })
    });
    let mut rig = rig(dir.path(), &[file], launcher);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));

    rig.sync();
    std::thread::sleep(Duration::from_millis(150));
    while let Ok(event) = rig.events.try_recv() {
        rig.app.handle_event(event);
    }

    assert!(rig.diagnostics().is_empty());
}

#[test]
fn reading_the_file_again_from_disk_clears_the_diagnostics() {
    let (mut rig, _received, _dir, file) = Rig::with_published(
        "fn main() {}\n",
        vec![diagnostic_json((0, 3), (0, 7), 1, "on main")],
    );

    fs::write(&file, "fn other() {}\n").unwrap();
    rig.app
        .handle_event(Event::FilesChanged(vec![file.clone()]));
    rig.sync();

    assert!(rig.diagnostics().is_empty());
}

#[test]
fn closing_the_tab_clears_its_diagnostics() {
    let (mut rig, _received, _dir, _file) = Rig::with_published(
        "fn main() {}\n",
        vec![diagnostic_json((0, 3), (0, 7), 1, "on main")],
    );
    let document = rig.front_document();

    rig.press(KeyCode::Char('w'), KeyModifiers::CONTROL);
    rig.sync();

    assert!(rig.app.state().diagnostics().of(document).is_empty());
}

#[test]
fn a_server_that_dies_takes_its_diagnostics_with_it() {
    let (dir, file) = rust_project();
    let (sent, _received) = std::sync::mpsc::channel::<Message>();
    // The first run publishes and then dies; the restarted one says nothing.
    let launcher = FakeLauncher::new(move |number| {
        let sent = sent.clone();
        if number == 0 {
            Some(
                Box::new(move |session: &mut crate::lsp::test_support::Session| {
                    let opened = session.next().expect("a didOpen");
                    let uri = params_of(&opened)["textDocument"]["uri"].clone();
                    session.notify(
                        "textDocument/publishDiagnostics",
                        publication(
                            &uri,
                            json!(0),
                            vec![diagnostic_json((0, 3), (0, 7), 1, "on main")],
                        ),
                    );
                    // A moment for it to be read before the connection ends.
                    std::thread::sleep(Duration::from_millis(100));
                }) as Script,
            )
        } else {
            responding(sent, |_| Vec::new())
        }
    });
    let mut rig = rig(dir.path(), &[file], launcher);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();
    rig.wait_until("the diagnostics", |rig| !rig.diagnostics().is_empty());

    rig.wait_until("the restart", |rig| rig.launcher.launched() >= 2);
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();

    assert!(rig.diagnostics().is_empty(), "{:?}", rig.diagnostics());
}

#[test]
fn changing_the_servers_in_the_settings_clears_the_diagnostics() {
    let (dir, file) = rust_project();
    let config = dir.path().join("config");
    let (sent, _received) = std::sync::mpsc::channel();
    let launcher = FakeLauncher::new(move |_| {
        responding(sent.clone(), |message| match method_of(message) {
            Some("textDocument/didOpen") => {
                let uri = params_of(message)["textDocument"]["uri"].clone();
                vec![publication(
                    &uri,
                    json!(0),
                    vec![diagnostic_json((0, 3), (0, 7), 1, "x")],
                )]
            }
            _ => Vec::new(),
        })
    });
    let mut rig = config_app(dir.path(), &file, launcher);
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    rig.sync();
    rig.wait_until("diagnostics", |rig| !rig.diagnostics().is_empty());

    reload_with(
        &mut rig,
        &config,
        r#"{ "language_servers": { "rust": { "command": "ra-multiplex", "extensions": ["rs"] } } }"#,
    );

    assert!(rig.diagnostics().is_empty());
}
