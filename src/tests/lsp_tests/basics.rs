use super::*;

#[test]
fn a_rust_file_in_front_starts_rust_analyzer_and_the_status_follows_it() {
    let (dir, file) = rust_project();
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| stays()));

    rig.look();
    assert_eq!(rig.status().as_deref(), Some("rust-analyzer: starting"));
    rig.wait_for_status(Some("rust-analyzer"));

    assert_eq!(rig.launcher.launched(), 1);
    assert!(rig.texts().is_empty(), "{:?}", rig.texts());
}

#[test]
fn the_server_is_started_for_the_root_of_the_project_not_the_folder_of_the_file() {
    let (dir, file) = rust_project();
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| stays()));

    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));

    let specs = rig.launcher.specs();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].command, "rust-analyzer");
    assert_eq!(specs[0].name, "rust");
    assert_eq!(
        crate::files::paths::without_verbatim_prefix(&crate::files::paths::canonical(
            &specs[0].root
        )),
        crate::files::paths::without_verbatim_prefix(&crate::files::paths::canonical(dir.path()))
    );
}

#[test]
fn the_status_shows_in_the_status_bar() {
    let (dir, file) = rust_project();
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| stays()));
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));

    let screen = rig.screen();

    assert!(screen.contains("[rust-analyzer]"), "{screen}");
}

#[test]
fn a_file_no_server_wants_has_no_status_and_starts_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    write(&file, "hello\n");
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| stays()));

    rig.look();

    assert_eq!(rig.status(), None);
    assert_eq!(rig.launcher.launched(), 0);
    assert!(!rig.screen().contains("[rust-analyzer"));
}

#[test]
fn two_rust_files_of_one_workspace_share_one_server() {
    let dir = tempfile::tempdir().unwrap();
    write(
        &dir.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\n",
    );
    write(&dir.path().join("crates/a/Cargo.toml"), "[package]\n");
    let a = dir.path().join("crates/a/src/lib.rs");
    write(&a, "");
    write(&dir.path().join("crates/b/Cargo.toml"), "[package]\n");
    let b = dir.path().join("crates/b/src/lib.rs");
    write(&b, "");
    let mut rig = rig(dir.path(), &[a, b], FakeLauncher::new(|_| stays()));

    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    rig.app
        .handle_input(InputEvent::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::CONTROL,
        )));
    rig.wait_for_status(Some("rust-analyzer"));

    assert_eq!(rig.launcher.launched(), 1);
}

#[test]
fn switching_to_a_file_with_no_server_clears_the_status_and_back_again_shows_it_without_a_new_start()
 {
    let (dir, rust) = rust_project();
    let notes = dir.path().join("notes.txt");
    write(&notes, "hello\n");
    let mut rig = rig(dir.path(), &[rust, notes], FakeLauncher::new(|_| stays()));
    rig.look();
    rig.wait_for_status(Some("rust-analyzer"));
    let next = || {
        InputEvent::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::CONTROL,
        ))
    };

    rig.app.handle_input(next());
    assert_eq!(rig.status(), None, "the notes have no server");
    rig.app.handle_input(next());

    assert_eq!(rig.status().as_deref(), Some("rust-analyzer"));
    assert_eq!(rig.launcher.launched(), 1);
}

#[test]
fn a_server_that_cannot_start_is_one_sticky_error_and_the_status_says_off() {
    let (dir, file) = rust_project();
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| None));

    rig.look();
    rig.wait_for_status(Some("rust-analyzer: off"));
    rig.look();
    rig.look();

    assert!(rig.app.state().notifications().has_errors());
    let texts = rig.texts();
    assert_eq!(texts.len(), 1, "{texts:?}");
    assert!(
        texts[0].contains("could not start") && texts[0].contains("was not found"),
        "{texts:?}"
    );
    assert_eq!(rig.launcher.launched(), 1, "not tried again");
}

#[test]
fn a_server_that_dies_is_restarted_and_the_user_is_told() {
    let (dir, file) = rust_project();
    let mut rig = rig(
        dir.path(),
        &[file],
        FakeLauncher::new(|number| if number == 0 { dies() } else { stays() }),
    );

    rig.look();
    let deadline = Instant::now() + Duration::from_secs(10);
    while rig.launcher.launched() < 2 || rig.status().as_deref() != Some("rust-analyzer") {
        while let Ok(event) = rig.events.try_recv() {
            rig.app.handle_event(event);
        }
        assert!(Instant::now() < deadline, "no restart: {:?}", rig.texts());
        std::thread::sleep(Duration::from_millis(2));
    }

    assert!(
        rig.texts()
            .iter()
            .any(|text| text.contains("restarting it (1 of 3)")),
        "{:?}",
        rig.texts()
    );
    assert!(!rig.app.state().notifications().has_errors());
}

#[test]
fn a_server_that_keeps_dying_ends_up_off_with_one_error() {
    let (dir, file) = rust_project();
    let mut rig = rig(dir.path(), &[file], FakeLauncher::new(|_| dies()));

    rig.look();
    rig.wait_for_status(Some("rust-analyzer: off"));

    assert_eq!(rig.launcher.launched(), 4);
    let errors: Vec<_> = rig
        .texts()
        .into_iter()
        .filter(|text| text.contains("stays off"))
        .collect();
    assert_eq!(errors.len(), 1, "{:?}", rig.texts());
    assert!(rig.app.state().notifications().has_errors());
}
