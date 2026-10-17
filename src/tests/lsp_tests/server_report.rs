use super::*;

#[test]
fn the_report_says_when_no_entry_lists_the_extension() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    write(&file, "text");
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| stays()));

    let report = rig.server_report();

    assert!(
        report.contains("notes.txt: no entry in language_servers lists the extension txt"),
        "{report}"
    );
}

#[test]
fn the_report_says_when_the_file_is_outside_the_project_and_has_no_root() {
    let project = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let file = elsewhere.path().join("src/lib.rs");
    write(&file, "");
    let mut rig = rig(project.path(), &[file], FakeLauncher::new(|_| stays()));

    let report = rig.server_report();

    assert!(report.contains("outside the project"), "{report}");
    assert!(report.contains("Cargo.toml"), "{report}");
}

#[test]
fn the_report_follows_the_server_from_not_started_to_open() {
    let (dir, file) = rust_project();
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| stays()));

    let before = rig.server_report();
    assert!(before.contains("has not been started"), "{before}");

    rig.look();
    let starting = rig.server_report();
    assert!(starting.contains("rust-analyzer: starting"), "{starting}");
    assert!(starting.contains("not running"), "{starting}");

    rig.wait_for_status(Some("rust-analyzer"));
    let running = rig.server_report();
    assert!(
        running.contains("the file is not open on it yet"),
        "{running}"
    );

    rig.app.lsp_sync();
    let open = rig.server_report();
    assert!(open.contains("main.rs: rust-analyzer"), "{open}");
    assert!(open.contains("the file is open on it"), "{open}");
    assert!(
        open.contains(
            &crate::files::paths::without_verbatim_prefix(&crate::files::paths::canonical(
                dir.path()
            ))
            .display()
            .to_string()
        ),
        "{open}"
    );
}

#[test]
fn the_report_of_a_server_that_is_off_says_so() {
    let (dir, file) = rust_project();
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| None));
    rig.look();
    rig.wait_for_status(Some("rust-analyzer: off"));

    let report = rig.server_report();

    assert!(report.contains("rust-analyzer: off"), "{report}");
    assert!(report.contains("not running"), "{report}");
}
