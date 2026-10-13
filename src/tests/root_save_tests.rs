//! Saving as root: the prompt, the password, and what `sudo` (a stand-in here)
//! does with it.
#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{Event as InputEvent, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tokio::sync::mpsc;

use crate::app::App;
use crate::components::prepare_and_render;
use crate::event::Event;
use crate::initial_state;
use crate::rootsave::RootWriter;
use crate::rootsave::tests::{can_write_anyway, fake_sudo, locked_folder};

struct Rig {
    app: App,
    events: mpsc::Receiver<Event>,
    file: PathBuf,
    /// Keeps the folders alive.
    _dirs: Vec<tempfile::TempDir>,
}

/// An app with `file` open, saving as root through the program `sudo`.
fn rig(file: PathBuf, sudo: &Path, dirs: Vec<tempfile::TempDir>) -> Rig {
    let staging = tempfile::tempdir().unwrap();
    let writer = RootWriter::with_program(sudo).staging_in(staging.path());
    let (tx, events) = mpsc::channel(64);
    let mut state = initial_state(std::slice::from_ref(&file)).unwrap();
    state.open_project(file.parent().unwrap()).unwrap();
    let app = App::new(state).with_events(tx).with_root_writer(writer);
    let mut dirs = dirs;
    dirs.push(staging);
    Rig {
        app,
        events,
        file,
        _dirs: dirs,
    }
}

/// A file in a folder the user may not write to, which the user may edit; or
/// `None` when the tests run as root and every folder can be written.
fn in_locked_folder(sudo_password: &str) -> Option<Rig> {
    let (dir, file) = locked_folder(Some("old\n"));
    if can_write_anyway(file.parent().unwrap()) {
        return None;
    }
    let sudo_dir = tempfile::tempdir().unwrap();
    let sudo = fake_sudo(sudo_dir.path(), sudo_password);
    Some(rig(file, &sudo, vec![dir, sudo_dir]))
}

impl Rig {
    fn press(&mut self, code: KeyCode) {
        self.key(code, KeyModifiers::NONE);
    }

    fn key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        self.app
            .handle_input(InputEvent::Key(KeyEvent::new(code, modifiers)));
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.press(KeyCode::Char(c));
        }
    }

    fn save(&mut self) {
        self.key(KeyCode::Char('s'), KeyModifiers::CONTROL);
    }

    fn screen(&mut self) -> String {
        let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
        let app = &mut self.app;
        terminal
            .draw(|frame| prepare_and_render(frame, app.state_mut()))
            .unwrap();
        terminal.backend().to_string()
    }

    fn prompt_open(&self) -> bool {
        self.app.state().root_prompt().is_some()
    }

    fn told(&self) -> String {
        self.app.state().notifications().texts().join("|")
    }

    fn text(&self) -> String {
        self.app.state().editor().buffer().text()
    }

    fn dirty(&self) -> bool {
        self.app.state().editor().buffer().is_dirty()
    }

    /// Takes what the threads sent until `done`.
    fn pump_until(&mut self, done: impl Fn(&Rig) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            while let Ok(event) = self.events.try_recv() {
                self.app.handle_event(event);
            }
            if done(self) {
                return;
            }
            assert!(Instant::now() < deadline, "it never happened");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Types a change, saves, and answers the prompt with `password`.
    fn save_with(&mut self, password: &str) {
        self.type_text("new ");
        self.save();
        assert!(self.prompt_open(), "{}", self.told());
        self.type_text(password);
        self.press(KeyCode::Enter);
    }
}

#[test]
fn a_save_the_user_may_not_make_opens_the_password_prompt_naming_the_file() {
    let Some(mut rig) = in_locked_folder("secret") else {
        return;
    };
    rig.type_text("new ");

    rig.save();

    assert!(rig.prompt_open());
    let screen = rig.screen();
    assert!(screen.contains("Save as root"), "{screen}");
    assert!(
        screen.contains(&format!("Save {} as root", rig.file.display())),
        "{screen}"
    );
    assert!(screen.contains("Password:"), "{screen}");
    // Nothing was written, and the buffer is still modified.
    assert_eq!(fs::read_to_string(&rig.file).unwrap(), "old\n");
    assert!(rig.dirty());
}

#[test]
fn the_password_is_shown_as_stars_and_never_as_itself() {
    let Some(mut rig) = in_locked_folder("secret") else {
        return;
    };
    rig.type_text("new ");
    rig.save();

    rig.type_text("hunter2");

    let screen = rig.screen();
    assert!(screen.contains("Password: *******"), "{screen}");
    assert!(!screen.contains("hunter2"), "{screen}");
    assert_eq!(rig.app.state().root_prompt().unwrap().stars(), 7);
    rig.press(KeyCode::Backspace);
    assert_eq!(rig.app.state().root_prompt().unwrap().stars(), 6);
    rig.key(KeyCode::Char('u'), KeyModifiers::CONTROL);
    assert_eq!(rig.app.state().root_prompt().unwrap().stars(), 0);
}

#[test]
fn the_right_password_saves_the_file_and_the_buffer_is_clean() {
    let Some(mut rig) = in_locked_folder("secret") else {
        return;
    };

    rig.save_with("secret");
    rig.pump_until(|rig| !rig.prompt_open());

    assert_eq!(fs::read_to_string(&rig.file).unwrap(), "new old\n");
    assert!(!rig.dirty());
    assert!(rig.told().contains("saved"), "{}", rig.told());
    assert!(rig.told().contains("as root"), "{}", rig.told());
    assert_eq!(fs::read_dir(rig.file.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn a_wrong_password_keeps_the_prompt_with_a_line_saying_so_and_empties_it() {
    let Some(mut rig) = in_locked_folder("secret") else {
        return;
    };

    rig.save_with("guess");
    rig.pump_until(|rig| {
        rig.app
            .state()
            .root_prompt()
            .is_some_and(|prompt| prompt.error().is_some())
    });

    assert!(rig.prompt_open());
    assert_eq!(rig.app.state().root_prompt().unwrap().stars(), 0);
    let screen = rig.screen();
    assert!(screen.contains("wrong password"), "{screen}");
    assert_eq!(fs::read_to_string(&rig.file).unwrap(), "old\n");

    // And it can be tried again, with the right one.
    rig.type_text("secret");
    rig.press(KeyCode::Enter);
    rig.pump_until(|rig| !rig.prompt_open());
    assert_eq!(fs::read_to_string(&rig.file).unwrap(), "new old\n");
}

#[test]
fn escape_gives_up_and_leaves_the_file_and_the_buffer_alone() {
    let Some(mut rig) = in_locked_folder("secret") else {
        return;
    };
    rig.type_text("new ");
    rig.save();
    rig.type_text("sec");

    rig.press(KeyCode::Esc);

    assert!(!rig.prompt_open());
    assert_eq!(fs::read_to_string(&rig.file).unwrap(), "old\n");
    assert!(rig.dirty());
    assert!(rig.told().contains("not saved"), "{}", rig.told());
}

#[test]
fn enter_with_nothing_typed_does_nothing() {
    let Some(mut rig) = in_locked_folder("secret") else {
        return;
    };
    rig.type_text("new ");
    rig.save();

    rig.press(KeyCode::Enter);

    assert!(rig.prompt_open());
    assert!(!rig.app.state().root_prompt().unwrap().is_waiting());
}

#[test]
fn keys_do_nothing_while_sudo_works_and_the_text_stays_for_what_it_was() {
    let (dir, file) = locked_folder(Some("old\n"));
    if can_write_anyway(file.parent().unwrap()) {
        return;
    }
    // A sudo that takes its time.
    let sudo_dir = tempfile::tempdir().unwrap();
    let real = fake_sudo(sudo_dir.path(), "secret");
    let slow = sudo_dir.path().join("slow-sudo");
    fs::write(
        &slow,
        format!(
            "#!/bin/sh\nIFS= read -r password\nsleep 0.4\nprintf '%s\\n' \"$password\" | exec {} \"$@\"\n",
            real.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&slow, fs::Permissions::from_mode(0o755)).unwrap();
    let mut rig = rig(file, &slow, vec![dir, sudo_dir]);
    rig.save_with("secret");
    assert!(rig.app.state().root_prompt().unwrap().is_waiting());

    // Typing goes nowhere, and an edit made behind its back keeps the buffer modified.
    rig.type_text("zzz");
    assert_eq!(rig.app.state().root_prompt().unwrap().stars(), 0);
    rig.app
        .state_mut()
        .edit(|editor| editor.insert_text("later ").unwrap());
    rig.pump_until(|rig| !rig.prompt_open());

    assert_eq!(fs::read_to_string(&rig.file).unwrap(), "new old\n");
    assert!(
        rig.dirty(),
        "what was typed after the save began is not saved"
    );
    assert!(rig.text().contains("later "), "{}", rig.text());
}

#[test]
fn a_user_sudo_does_not_allow_is_told_once_and_not_asked_again() {
    let (dir, file) = locked_folder(Some("old\n"));
    if can_write_anyway(file.parent().unwrap()) {
        return;
    }
    let sudo_dir = tempfile::tempdir().unwrap();
    let sudo = sudo_dir.path().join("refusing-sudo");
    fs::write(
        &sudo,
        "#!/bin/sh\nread -r password\necho 'tim is not in the sudoers file.' >&2\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&sudo, fs::Permissions::from_mode(0o755)).unwrap();
    let mut rig = rig(file, &sudo, vec![dir, sudo_dir]);

    rig.save_with("anything");
    rig.pump_until(|rig| !rig.prompt_open());

    assert!(rig.told().contains("does not let you"), "{}", rig.told());
    // The next save says it at once.
    rig.save();
    assert!(!rig.prompt_open());
    assert_eq!(fs::read_to_string(&rig.file).unwrap(), "old\n");
}

#[test]
fn without_sudo_it_says_so_once_and_does_not_ask_again() {
    let (dir, file) = locked_folder(Some("old\n"));
    if can_write_anyway(file.parent().unwrap()) {
        return;
    }
    let mut rig = rig(file, Path::new("/nonexistent/sudo"), vec![dir]);

    rig.save_with("x");
    rig.pump_until(|rig| !rig.prompt_open());

    assert!(
        rig.told().contains("sudo is not installed"),
        "{}",
        rig.told()
    );
    rig.save();
    assert!(!rig.prompt_open());
}

#[test]
fn a_file_the_app_locked_on_purpose_is_never_saved_as_root() {
    let Some(mut rig) = in_locked_folder("secret") else {
        return;
    };
    let document = rig.app.state().workspace().active_document_id().unwrap();
    rig.app.state_mut().workspace_mut().lock_document(document);

    rig.save();

    assert!(!rig.prompt_open());
    assert!(rig.told().contains("read-only"), "{}", rig.told());
}

// ---- a file that is read-only to the user

#[test]
fn a_read_only_file_is_locked_until_edit_anyway_and_then_saves_as_root() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("hosts");
    fs::write(&file, "old\n").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o444)).unwrap();
    let sudo_dir = tempfile::tempdir().unwrap();
    let sudo = fake_sudo(sudo_dir.path(), "secret");
    let mut rig = rig(file, &sudo, vec![dir, sudo_dir]);

    rig.type_text("new ");
    assert_eq!(rig.text(), "old\n", "the text is locked");

    // The palette's "File: Edit Anyway".
    rig.key(KeyCode::F(1), KeyModifiers::NONE);
    rig.type_text("Edit Anyway");
    rig.press(KeyCode::Enter);
    assert!(rig.told().contains("editable"), "{}", rig.told());
    rig.save_with("secret");
    rig.pump_until(|rig| !rig.prompt_open());

    assert_eq!(fs::read_to_string(&rig.file).unwrap(), "new old\n");
    assert_eq!(
        fs::metadata(&rig.file).unwrap().permissions().mode() & 0o777,
        0o444,
        "the mode of the file is kept"
    );
    assert!(!rig.dirty());
}

#[test]
fn a_file_the_app_locked_cannot_be_unlocked_for_editing() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("x.rs");
    fs::write(&file, "old\n").unwrap();
    let sudo = Path::new("/nonexistent/sudo");
    let mut rig = rig(file, sudo, vec![dir]);
    let document = rig.app.state().workspace().active_document_id().unwrap();
    rig.app.state_mut().workspace_mut().lock_document(document);

    rig.key(KeyCode::F(1), KeyModifiers::NONE);
    rig.type_text("Edit Anyway");
    rig.press(KeyCode::Enter);

    assert!(
        rig.told().contains("locked it on purpose"),
        "{}",
        rig.told()
    );
    rig.type_text("x");
    assert_eq!(rig.text(), "old\n");
}

#[test]
fn the_quit_prompt_says_how_to_save_as_root_instead_of_asking_for_a_password() {
    let Some(mut rig) = in_locked_folder("secret") else {
        return;
    };
    rig.type_text("new ");
    rig.key(KeyCode::Char('q'), KeyModifiers::CONTROL);
    rig.key(KeyCode::Char('s'), KeyModifiers::CONTROL);

    assert!(!rig.prompt_open());
    assert!(rig.told().contains("save it as root"), "{}", rig.told());
}
