use super::*;

/// A temporary project with a type error on line 2 (counted from 0) of a `.ts` file and of a
/// `.tsx` file, opened in the app with the real launcher: the default `javascript` entry serves
/// both, so after the server has looked each file has the error on its line. Run by hand, since
/// CI has no `typescript-language-server`:
/// `cargo test real_typescript_server -- --ignored --nocapture`.
#[test]
#[ignore = "needs typescript-language-server on the PATH; run by hand"]
fn real_typescript_server_reports_a_type_error_on_the_right_line_of_ts_and_tsx_files() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("tsconfig.json"),
        "{ \"compilerOptions\": { \"strict\": true, \"jsx\": \"preserve\", \"target\": \"es2020\" } }\n",
    );
    let ts = dir.path().join("main.ts");
    write(
        &ts,
        "interface Box { size: number }\nconst ok: Box = { size: 1 };\nconst wrong: number = \"not a number\";\nexport { ok, wrong };\n",
    );
    let tsx = dir.path().join("view.tsx");
    write(
        &tsx,
        "const view = <div>hi</div>;\nconst fine: string = \"x\";\nconst wrong: number = \"not a number\";\nexport { view, fine, wrong };\n",
    );

    let (tx, mut events) = mpsc::channel(256);
    let mut app = App::new(opened(dir.path(), &[ts.clone(), tsx.clone()]))
        .with_events(tx)
        .with_lsp_launcher(Arc::new(crate::lsp::ProcessLauncher));
    app.handle_input(InputEvent::FocusLost);

    let both = |app: &App| {
        ["main.ts", "view.tsx"].into_iter().all(|name| {
            let workspace = app.state().workspace();
            workspace
                .documents_with_paths()
                .into_iter()
                .find(|(_, path)| path.ends_with(name))
                .is_some_and(|(document, _)| !app.state().diagnostics().of(document).is_empty())
        })
    };
    let deadline = Instant::now() + Duration::from_secs(120);
    while !both(&app) {
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

    for name in ["main.ts", "view.tsx"] {
        let workspace = app.state().workspace();
        let (document, _) = workspace
            .documents_with_paths()
            .into_iter()
            .find(|(_, path)| path.ends_with(name))
            .unwrap();
        let (buffer, lines) = {
            let (_, _, buffer) = workspace.document_info(document).unwrap();
            (
                buffer,
                app.state()
                    .diagnostics()
                    .of(document)
                    .iter()
                    .map(|diagnostic| {
                        let start = diagnostic.range.start;
                        (
                            buffer.byte_to_position(start).map(|at| at.line),
                            diagnostic.message.clone(),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        };
        let _ = buffer;
        assert!(
            lines
                .iter()
                .any(|(line, message)| *line == Ok(2) && message.contains("not assignable")),
            "{name}: the type error is on line 2: {lines:?}"
        );
    }
}
