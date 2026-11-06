//! "Open in Terminal", "Open in External Terminal" and "Open in File Manager": the folder is
//! the selected entry (or the folder of the file), and a failure says so.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;

use super::*;
use crate::event::action::Action;

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub/inner.txt"), "x\n").unwrap();
    fs::write(dir.path().join("top.txt"), "y\n").unwrap();
    dir
}

impl Rig {
    /// Moves the keyboard to the explorer and selects `path`.
    fn select_in_explorer(&mut self, path: &Path) {
        self.app.state_mut().set_focus(Focus::Explorer);
        self.app.state_mut().explorer_mut().reveal(path).unwrap();
        assert_eq!(
            self.app.state().explorer().selected_row().unwrap().path,
            path
        );
    }
}
/// A script that records its arguments and its folder in `record.txt` next to it.
fn recorder(dir: &Path) -> String {
    let script = dir.join("recorder.sh");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\necho \"$@ @ $(pwd)\" >> {}/record.txt\n",
            dir.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    script.display().to_string()
}

fn recorded(dir: &Path) -> String {
    let file = dir.join("record.txt");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !file.exists() {
        assert!(Instant::now() < deadline, "nothing was started");
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(50));
    fs::read_to_string(file).unwrap()
}

#[test]
fn open_in_terminal_starts_a_shell_in_the_selected_folder() {
    let dir = project();
    let mut rig = rig_in(dir.path(), "pwd; sleep 30");
    rig.select_in_explorer(&dir.path().join("sub"));

    rig.app.run_action(Action::OpenInTerminal);

    let sub = dir.path().join("sub").canonicalize().unwrap();
    rig.wait_for_text(&sub.display().to_string());
    assert_eq!(rig.app.state().focus(), Focus::Terminal);
}

#[test]
fn open_in_terminal_on_a_file_uses_the_folder_it_is_in() {
    let dir = project();
    let mut rig = rig_in(dir.path(), "pwd; sleep 30");
    rig.select_in_explorer(&dir.path().join("sub/inner.txt"));

    rig.app.run_action(Action::OpenInTerminal);

    let sub = dir.path().join("sub").canonicalize().unwrap();
    rig.wait_for_text(&sub.display().to_string());
}

#[test]
fn open_in_external_terminal_starts_the_setting_with_the_folder() {
    let dir = project();
    let mut rig = rig_in(dir.path(), "sleep 30");
    let script = recorder(dir.path());
    rig.app.settings_mut().terminal = Some(vec![script, "--at".to_owned(), "${folder}".to_owned()]);
    rig.select_in_explorer(&dir.path().join("sub"));

    rig.app.run_action(Action::OpenInExternalTerminal);

    let sub = dir.path().join("sub");
    let record = recorded(dir.path());
    assert_eq!(
        record.trim(),
        format!("--at {0} @ {0}", sub.display()),
        "the folder is an argument, and the working folder"
    );
}

#[test]
fn an_external_terminal_that_is_not_installed_says_how_to_set_one() {
    let dir = project();
    let mut rig = rig_in(dir.path(), "sleep 30");
    rig.app.settings_mut().terminal = Some(vec!["/nonexistent/terminal".to_owned()]);

    rig.app.run_action(Action::OpenInExternalTerminal);

    assert!(
        rig.notifications().contains("no terminal program found"),
        "{}",
        rig.notifications()
    );
    assert!(rig.notifications().contains("`terminal`"));
}

#[test]
fn open_in_file_manager_gives_the_folder_to_the_opener() {
    let dir = project();
    let mut rig = rig_in(dir.path(), "sleep 30");
    let script = recorder(dir.path());
    let app = std::mem::take(&mut rig.app);
    rig.app = app.with_opener(&script);
    rig.select_in_explorer(&dir.path().join("top.txt"));

    rig.app.run_action(Action::OpenInFileManager);

    let record = recorded(dir.path());
    assert!(
        record.trim().starts_with(&dir.path().display().to_string()),
        "the folder of top.txt, not the file: {record}"
    );
    assert!(!record.contains("top.txt"));
}

#[test]
fn with_no_file_the_project_folder_is_used() {
    let dir = project();
    let mut rig = rig_in(dir.path(), "sleep 30");
    let script = recorder(dir.path());
    let app = std::mem::take(&mut rig.app);
    rig.app = app.with_opener(&script);
    rig.app.state_mut().set_focus(Focus::Editor);

    rig.app.run_action(Action::OpenInFileManager);

    assert!(recorded(dir.path()).contains(&dir.path().display().to_string()));
}
