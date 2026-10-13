use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::{RootError, RootWriter, Secret, classify};

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
