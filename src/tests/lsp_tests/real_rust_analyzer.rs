use super::*;

/// A temporary crate with a type error on line 3 (counted from 0), opened in the
/// app with the real launcher: after the server has looked, the error is said
/// after the code on its line, underlined under the string, and
/// `f8` goes to it. Run by hand, since CI has no `rust-analyzer`:
/// `cargo test real_rust_analyzer_errors_show -- --ignored --nocapture`.
#[test]
#[ignore = "needs rust-analyzer on the PATH; run by hand"]
fn real_rust_analyzer_errors_show_on_the_right_lines_in_the_app() {
    use ratatui::style::Modifier;

    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("Cargo.toml"),
        "[package]\nname = \"small\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    let text = "fn main() {\n    let ok: u32 = 1;\n    let note = \"😀 fine\";\n    let wrong: u32 = \"not a number\";\n    println!(\"{ok} {note} {wrong}\");\n}\n";
    let file = dir.path().join("src/main.rs");
    write(&file, text);

    let (tx, mut events) = mpsc::channel(256);
    let mut app = App::new(opened(dir.path(), std::slice::from_ref(&file)))
        .with_events(tx)
        .with_lsp_launcher(Arc::new(crate::lsp::ProcessLauncher));
    app.handle_input(InputEvent::FocusLost);
    let document = app.state().workspace().active_document_id().unwrap();

    let deadline = Instant::now() + Duration::from_secs(120);
    while app.state().diagnostics().of(document).is_empty() {
        while let Ok(event) = events.try_recv() {
            app.handle_event(event);
        }
        // What the loop does before each frame.
        app.lsp_sync();
        assert!(
            Instant::now() < deadline,
            "no diagnostics in two minutes; status {:?}, notices {:?}",
            app.state().lsp_status(),
            app.state().notifications().texts()
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    let draw = |app: &mut App| {
        let mut terminal = Terminal::new(TestBackend::new(120, 14)).unwrap();
        terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
        terminal.backend().buffer().clone()
    };
    let row_with = |screen: &ratatui::buffer::Buffer, needle: &str| -> u16 {
        (0..screen.area.height)
            .find(|row| {
                (0..screen.area.width)
                    .map(|column| screen[(column, *row)].symbol().to_owned())
                    .collect::<String>()
                    .contains(needle)
            })
            .unwrap_or_else(|| panic!("no row shows {needle:?}"))
    };

    // Go to the error with f8, as a user would.
    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::F(8),
        KeyModifiers::NONE,
    )));
    let screen = draw(&mut app);

    let row = row_with(&screen, "let wrong");
    let cells: Vec<_> = (0..screen.area.width)
        .map(|column| &screen[(column, row)])
        .collect();
    let string_start = cells
        .iter()
        .position(|cell| cell.symbol() == "\"")
        .expect("the string is on the row");
    let underlined: Vec<bool> = cells
        .iter()
        .map(|cell| cell.modifier.contains(Modifier::UNDERLINED))
        .collect();
    eprintln!(
        "underlined cells: {:?}",
        underlined
            .iter()
            .enumerate()
            .filter(|(_, on)| **on)
            .map(|(at, _)| at)
            .collect::<Vec<_>>()
    );
    let line: String = cells.iter().map(|cell| cell.symbol()).collect();
    eprintln!("the line: {}", line.trim_end());
    assert!(
        line.contains("expected u32"),
        "the message is after the code, on the line with the error: {line}"
    );
    assert!(!line.contains('●'), "and nothing looks like a breakpoint");
    assert!(underlined[string_start], "the string is underlined");
    assert!(
        !underlined[string_start - 2],
        "and what is before it is not"
    );
    assert_eq!(
        app.state().editor().selections().primary().head().line,
        3,
        "f8 went to the line with the error"
    );

    // f7 shows the whole message in a box.
    app.handle_input(InputEvent::Key(KeyEvent::new(
        KeyCode::F(7),
        KeyModifiers::NONE,
    )));
    let boxed = draw(&mut app);
    let box_text: String = (0..boxed.area.height)
        .map(|row| {
            (0..boxed.area.width)
                .map(|column| boxed[(column, row)].symbol().to_owned())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    eprintln!("{box_text}");
    assert!(box_text.contains("Diagnostics"), "the box is open");
    assert!(
        box_text.contains("expected u32, found"),
        "with the whole message in it"
    );
}

/// The same in a Cargo workspace, opened at its top with a file in a member
/// crate: the server must start at the workspace's `Cargo.toml` and report the
/// member's error. Run by hand like the one above.
#[test]
#[ignore = "needs rust-analyzer on the PATH; run by hand"]
fn real_rust_analyzer_works_in_a_cargo_workspace() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = [\"crates/api\", \"crates/importer\"]\n\n[workspace.package]\nedition = \"2021\"\n",
    );
    for member in ["api", "importer"] {
        write(
            &dir.path().join(format!("crates/{member}/Cargo.toml")),
            &format!(
                "[package]\nname = \"{member}\"\nversion = \"0.1.0\"\nedition.workspace = true\n"
            ),
        );
    }
    write(
        &dir.path().join("crates/importer/src/main.rs"),
        "fn main() {}\n",
    );
    let file = dir.path().join("crates/api/src/main.rs");
    write(&file, "fn main() {\n    let wrong: u32 = \"text\";\n}\n");

    let (tx, mut events) = mpsc::channel(256);
    let mut app = App::new(opened(dir.path(), std::slice::from_ref(&file)))
        .with_events(tx)
        .with_lsp_launcher(Arc::new(crate::lsp::ProcessLauncher));
    app.handle_input(InputEvent::FocusLost);
    let document = app.state().workspace().active_document_id().unwrap();

    let deadline = Instant::now() + Duration::from_secs(120);
    while app.state().diagnostics().of(document).is_empty() {
        while let Ok(event) = events.try_recv() {
            app.handle_event(event);
        }
        app.lsp_sync();
        assert!(
            Instant::now() < deadline,
            "no diagnostics in two minutes; status {:?}, notices {:?}",
            app.state().lsp_status(),
            app.state().notifications().texts()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A crate inside the project that the project's own `Cargo.toml` does not know:
/// `cargo test real_rust_analyzer_in_a_nested_crate -- --ignored --nocapture`.
#[test]
#[ignore = "needs rust-analyzer on the PATH; run by hand"]
fn real_rust_analyzer_in_a_nested_crate_that_is_not_a_member_of_the_project() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("Cargo.toml"),
        "[package]\nname = \"outer\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(&dir.path().join("src/main.rs"), "fn main() {}\n");
    write(
        &dir.path().join("lib/inner/Cargo.toml"),
        "[package]\nname = \"inner\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    let file = dir.path().join("lib/inner/src/lib.rs");
    write(&file, "pub fn f() {\n    let wrong: u32 = \"text\";\n}\n");

    let (tx, mut events) = mpsc::channel(256);
    let mut app = App::new(opened(dir.path(), std::slice::from_ref(&file)))
        .with_events(tx)
        .with_lsp_launcher(Arc::new(crate::lsp::ProcessLauncher));
    app.handle_input(InputEvent::FocusLost);
    let document = app.state().workspace().active_document_id().unwrap();

    let deadline = Instant::now() + Duration::from_secs(90);
    while app.state().diagnostics().of(document).is_empty() {
        while let Ok(event) = events.try_recv() {
            app.handle_event(event);
        }
        app.lsp_sync();
        assert!(
            Instant::now() < deadline,
            "no diagnostics in 90 s; status {:?}, notices {:?}",
            app.state().lsp_status(),
            app.state().notifications().texts()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A nested crate that the project's `Cargo.toml` depends on by path:
/// `cargo test real_rust_analyzer_in_a_nested_crate_the_project_depends_on_by_path -- --ignored --nocapture`.
#[test]
#[ignore = "needs rust-analyzer on the PATH; run by hand"]
fn real_rust_analyzer_in_a_nested_crate_the_project_depends_on_by_path() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("Cargo.toml"),
        "[package]\nname = \"outer\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ninner = { path = \"lib/inner\" }\n",
    );
    write(&dir.path().join("src/main.rs"), "fn main() {}\n");
    write(
        &dir.path().join("lib/inner/Cargo.toml"),
        "[package]\nname = \"inner\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    let file = dir.path().join("lib/inner/src/lib.rs");
    write(&file, "pub fn f() {\n    let wrong: u32 = \"text\";\n}\n");

    let (tx, mut events) = mpsc::channel(256);
    let mut app = App::new(opened(dir.path(), std::slice::from_ref(&file)))
        .with_events(tx)
        .with_lsp_launcher(Arc::new(crate::lsp::ProcessLauncher));
    app.handle_input(InputEvent::FocusLost);
    let document = app.state().workspace().active_document_id().unwrap();

    let deadline = Instant::now() + Duration::from_secs(90);
    while app.state().diagnostics().of(document).is_empty() {
        while let Ok(event) = events.try_recv() {
            app.handle_event(event);
        }
        app.lsp_sync();
        assert!(
            Instant::now() < deadline,
            "no diagnostics in 90 s; status {:?}, notices {:?}",
            app.state().lsp_status(),
            app.state().notifications().texts()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
