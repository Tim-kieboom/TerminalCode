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
use crate::files::rootsave::{RootError, RootWriter, Secret, classify};
use crate::initial_state;

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

fn secret(text: &str) -> Secret {
    let mut secret = Secret::default();
    for c in text.chars() {
        secret.push(c);
    }
    secret
}

/// A `sudo` that stands in for the real one, in `dir`: it reads the password
/// from its input, says "Sorry, try again." when it is not `expected`, and
/// otherwise runs the command after the options as the user does, after making
/// the folder of the target writable for the moment (what being root allows).
pub(crate) fn fake_sudo(dir: &Path, expected: &str) -> PathBuf {
    let path = dir.join("fake-sudo");
    let bin = dir.join("fakebin");
    fs::create_dir_all(&bin).unwrap();
    // Being root is what lets `chown` give a file to root; a stand-in does nothing.
    fs::write(bin.join("chown"), "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(bin.join("chown"), fs::Permissions::from_mode(0o755)).unwrap();
    let script = format!(
        r#"#!/bin/sh
read -r password
if [ "$password" != '{expected}' ]; then
  echo "Sorry, try again." >&2
  exit 1
fi
# -S -k -p '' sh -c SCRIPT sh SRC DST
shift 4
for last; do :; done
folder=$(dirname "$last")
chmod u+w "$folder"
PATH='{bin}':$PATH
export PATH
"$@"
status=$?
chmod a-w "$folder"
exit $status
"#,
        bin = bin.display()
    );
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// A folder the user may not write to, with a file in it when `existing` says so.
pub(crate) fn locked_folder(existing: Option<&str>) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("etc");
    fs::create_dir(&folder).unwrap();
    let file = folder.join("hosts");
    if let Some(text) = existing {
        fs::write(&file, text).unwrap();
    }
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).unwrap();
    (dir, file)
}

/// Whether the user can write into the folder anyway (running as root).
pub(crate) fn can_write_anyway(folder: &Path) -> bool {
    let probe = folder.join("probe");
    let wrote = fs::write(&probe, "").is_ok();
    let _ = fs::remove_file(&probe);
    wrote
}

/// A writer for `program` that stages in a folder of its own, which it gives back too.
fn writer(program: impl AsRef<std::ffi::OsStr>) -> (RootWriter, PathBuf) {
    let staging = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    let path = staging.path().to_path_buf();
    (
        RootWriter::with_program(program.as_ref()).staging_in(&path),
        path,
    )
}

fn left_over(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

// ---- the secret

#[test]
fn a_secret_counts_characters_and_takes_the_last_one_away() {
    let mut password = secret("pässword");

    assert_eq!(password.chars(), 8);
    password.pop();
    assert_eq!(password.chars(), 7);
    assert_eq!(password.raw_for_test(), "pässwor".as_bytes());
    for _ in 0..20 {
        password.pop();
    }
    assert!(password.is_empty());
}

#[test]
fn clearing_a_secret_overwrites_its_bytes() {
    let mut password = secret("hunter2");
    let before = password.raw_for_test().as_ptr();

    password.clear();

    assert!(password.is_empty());
    // The memory it held is zeros now (the vector keeps its capacity).
    // SAFETY: the buffer is still owned by `password`, which is alive.
    let held = unsafe { std::slice::from_raw_parts(before, 7) };
    assert_eq!(held, [0; 7]);
}

#[test]
fn a_secret_does_not_show_itself_in_debug_output() {
    assert_eq!(format!("{:?}", secret("hunter2")), "Secret(..)");
}

// ---- what sudo says

#[test]
fn what_sudo_says_is_sorted_into_what_went_wrong() {
    assert_eq!(classify("Sorry, try again.\n"), RootError::WrongPassword);
    assert_eq!(
        classify("sudo: 1 incorrect password attempt"),
        RootError::WrongPassword
    );
    assert_eq!(
        classify("sudo: no password was provided"),
        RootError::WrongPassword
    );
    assert_eq!(
        classify("tim is not in the sudoers file.\n"),
        RootError::NotAllowed("tim is not in the sudoers file.".to_owned())
    );
    assert_eq!(
        classify("\nmv: cannot move 'a' to 'b': Device or resource busy\n"),
        RootError::Failed("mv: cannot move 'a' to 'b': Device or resource busy".to_owned())
    );
    assert_eq!(
        classify(""),
        RootError::Failed("it stopped without saying why".to_owned())
    );
}

#[test]
fn every_error_has_something_to_tell_the_user() {
    for error in [
        RootError::NoSudo,
        RootError::NotAllowed("no".to_owned()),
        RootError::WrongPassword,
        RootError::Failed("no".to_owned()),
    ] {
        assert!(!error.message().is_empty());
    }
}

// ---- writing

#[test]
fn a_new_file_in_a_folder_the_user_cannot_write_is_made() {
    let (dir, file) = locked_folder(None);
    if can_write_anyway(file.parent().unwrap()) {
        return;
    }
    let sudo = fake_sudo(dir.path(), "secret");

    let (writer, staging) = writer(&sudo);
    writer
        .write(&secret("secret"), &file, "127.0.0.1 localhost\n")
        .unwrap();

    assert_eq!(fs::read_to_string(&file).unwrap(), "127.0.0.1 localhost\n");
    // The folder is locked again and holds nothing else.
    assert_eq!(left_over(file.parent().unwrap()), ["hosts"]);
    assert_eq!(
        fs::metadata(file.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o222,
        0
    );
    assert_eq!(left_over(&staging), Vec::<String>::new());
}

#[test]
fn an_existing_file_is_replaced_and_keeps_its_mode() {
    let (dir, file) = locked_folder(Some("old\n"));
    fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
    if can_write_anyway(file.parent().unwrap()) {
        return;
    }
    let sudo = fake_sudo(dir.path(), "secret");

    RootWriter::with_program(&sudo)
        .write(&secret("secret"), &file, "new\n")
        .unwrap();

    assert_eq!(fs::read_to_string(&file).unwrap(), "new\n");
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(left_over(file.parent().unwrap()), ["hosts"]);
}

#[test]
fn a_file_that_is_read_only_to_the_user_is_replaced_too() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("hosts");
    fs::write(&file, "old\n").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o444)).unwrap();
    let sudo = fake_sudo(dir.path(), "secret");

    RootWriter::with_program(&sudo)
        .write(&secret("secret"), &file, "new\n")
        .unwrap();

    assert_eq!(fs::read_to_string(&file).unwrap(), "new\n");
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o444
    );
}

#[test]
fn a_wrong_password_changes_nothing_and_says_so() {
    let (dir, file) = locked_folder(Some("old\n"));
    let sudo = fake_sudo(dir.path(), "secret");

    let (writer, staging) = writer(&sudo);
    let result = writer.write(&secret("guess"), &file, "new\n");

    assert_eq!(result, Err(RootError::WrongPassword));
    assert_eq!(fs::read_to_string(&file).unwrap(), "old\n");
    assert_eq!(left_over(&staging), Vec::<String>::new());
}

#[test]
fn a_program_that_is_not_there_is_no_sudo() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("hosts");

    let (writer, staging) = writer("/nonexistent/sudo");
    let result = writer.write(&secret("x"), &file, "new\n");

    assert_eq!(result, Err(RootError::NoSudo));
    assert_eq!(left_over(&staging), Vec::<String>::new());
}

#[test]
fn a_script_that_fails_says_what_it_said_and_leaves_no_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let sudo = dir.path().join("failing-sudo");
    fs::write(
        &sudo,
        "#!/bin/sh\nread -r password\necho 'mv: it broke' >&2\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&sudo, fs::Permissions::from_mode(0o755)).unwrap();

    let (writer, staging) = writer(&sudo);
    let result = writer.write(&secret("x"), &dir.path().join("f"), "t");

    assert_eq!(result, Err(RootError::Failed("mv: it broke".to_owned())));
    assert_eq!(left_over(&staging), Vec::<String>::new());
}

#[test]
fn the_text_is_staged_where_only_the_user_can_read_it() {
    let dir = tempfile::tempdir().unwrap();
    let sudo = dir.path().join("looking-sudo");
    // Says the mode of the staged file (its first path argument) and fails.
    fs::write(
        &sudo,
        "#!/bin/sh\nread -r password\nshift 8\nstat -c %a \"$1\" >&2\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&sudo, fs::Permissions::from_mode(0o755)).unwrap();

    let result = RootWriter::with_program(&sudo).write(&secret("x"), &dir.path().join("f"), "t");

    assert_eq!(result, Err(RootError::Failed("600".to_owned())));
}

#[test]
fn the_password_goes_to_sudo_on_its_input_not_in_its_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let sudo = dir.path().join("arguments-sudo");
    let seen = dir.path().join("arguments");
    fs::write(
        &sudo,
        format!(
            "#!/bin/sh\nread -r password\necho \"$password\" > {input}\necho \"$@\" > {seen}\nexit 1\n",
            input = dir.path().join("input").display(),
            seen = seen.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&sudo, fs::Permissions::from_mode(0o755)).unwrap();

    let _ = RootWriter::with_program(&sudo).write(&secret("tr0ub4dor"), &dir.path().join("f"), "t");

    assert_eq!(
        fs::read_to_string(dir.path().join("input")).unwrap(),
        "tr0ub4dor\n"
    );
    assert!(!fs::read_to_string(&seen).unwrap().contains("tr0ub4dor"));
    // `-k` makes sudo ask every time and `-S` makes it read the password from the input.
    let arguments = fs::read_to_string(&seen).unwrap();
    assert!(arguments.starts_with("-S -k -p"), "{arguments}");
}

/// The real `sudo`, by hand: writes a file in `/etc`, which only root may do,
/// checks who owns it and removes it again.
/// `TRCODE_SUDO_PASSWORD=... cargo test the_real_sudo -- --ignored --nocapture`.
#[test]
#[ignore = "runs the real sudo and needs TRCODE_SUDO_PASSWORD; run by hand"]
fn the_real_sudo_writes_a_file_in_a_folder_only_root_can_write() {
    use std::io::Write;
    use std::os::unix::fs::MetadataExt;
    use std::process::{Command, Stdio};

    let password = std::env::var("TRCODE_SUDO_PASSWORD").expect("TRCODE_SUDO_PASSWORD is not set");
    let target = PathBuf::from(format!("/etc/trcode-root-save-test-{}", std::process::id()));

    let wrong = RootWriter::default().write(&secret("not the password"), &target, "x\n");
    assert_eq!(wrong, Err(RootError::WrongPassword));
    RootWriter::default()
        .write(&secret(&password), &target, "hello\n")
        .unwrap();

    assert_eq!(fs::read_to_string(&target).unwrap(), "hello\n");
    let metadata = fs::metadata(&target).unwrap();
    assert_eq!(metadata.uid(), 0, "the file belongs to root");
    assert_eq!(metadata.permissions().mode() & 0o777, 0o644);
    // Replacing it keeps what it is.
    RootWriter::default()
        .write(&secret(&password), &target, "again\n")
        .unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "again\n");
    assert_eq!(fs::metadata(&target).unwrap().uid(), 0);

    let mut remove = Command::new("sudo")
        .args(["-S", "-k", "-p", "", "rm", "-f"])
        .arg(&target)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(remove.stdin.take().unwrap(), "{password}").unwrap();
    assert!(remove.wait().unwrap().success());
    assert!(!target.exists());
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
