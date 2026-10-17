//! Saving a file as root (Linux): when a save fails because the user may not
//! write the file or its folder, the text goes to a temporary file as the user
//! (mode 0600, removed on every path), and one `sudo` call puts it in place:
//! a temporary name in the target's folder, the owner and mode of the file it
//! replaces (a new file gets `root:root 0644`), then `mv` over the target, which
//! is atomic. The password is typed in a prompt, goes to `sudo -S` on its
//! standard input and is wiped from memory at once; `-k` makes `sudo` ask every
//! time, so nothing is cached. See "Save as root" in `docs/todo.md`.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A secret that is wiped when it is dropped: the bytes are overwritten with
/// zeros in a way the compiler may not leave out.
#[derive(Default)]
pub struct Secret {
    bytes: Vec<u8>,
}

impl Secret {
    pub fn push(&mut self, c: char) {
        let mut buffer = [0; 4];
        self.bytes
            .extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
        // The temporary copy of the character is wiped too.
        wipe(&mut buffer);
    }

    /// Takes the last character away.
    pub fn pop(&mut self) {
        let Some(last) = std::str::from_utf8(&self.bytes)
            .ok()
            .and_then(|text| text.chars().next_back())
        else {
            return;
        };
        let keep = self.bytes.len() - last.len_utf8();
        wipe(&mut self.bytes[keep..]);
        self.bytes.truncate(keep);
    }

    /// How many characters it has, for showing that many stars.
    pub fn chars(&self) -> usize {
        std::str::from_utf8(&self.bytes).map_or(self.bytes.len(), |text| text.chars().count())
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Overwrites it with zeros and empties it.
    pub fn clear(&mut self) {
        wipe(&mut self.bytes);
        self.bytes.clear();
    }

    pub fn raw_for_test(&self) -> &[u8] {
        &self.bytes
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.clear();
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(..)")
    }
}

/// Writes zeros over `bytes`, so that the compiler cannot see the writes are never read.
fn wipe(bytes: &mut [u8]) {
    for byte in bytes.iter_mut() {
        // SAFETY: `byte` is a valid, aligned pointer to a `u8` we hold exclusively.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    std::sync::atomic::fence(std::sync::atomic::Ordering::SeqCst);
}

/// Why a save as root did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootError {
    /// There is no `sudo` to run. Not tried again.
    NoSudo,
    /// `sudo` refuses this user. Not tried again.
    NotAllowed(String),
    /// The password was not right: the prompt asks again.
    WrongPassword,
    /// The script or `sudo` failed for another reason, with what it said.
    Failed(String),
}

impl RootError {
    /// What the user is told, for the errors that end the attempt.
    pub fn message(&self) -> String {
        match self {
            Self::NoSudo => "sudo is not installed, so this cannot be saved as root".to_owned(),
            Self::NotAllowed(said) => format!("sudo does not let you save as root: {said}"),
            Self::WrongPassword => "wrong password".to_owned(),
            Self::Failed(said) => format!("saving as root failed: {said}"),
        }
    }
}

/// The script `sudo` runs. `$1` is the file with the new text, `$2` the target.
const SCRIPT: &str = r#"set -e
src=$1
dst=$2
tmp=$(mktemp "$dst.trcode.XXXXXX")
trap 'rm -f "$tmp"' EXIT
cat "$src" > "$tmp"
if [ -e "$dst" ]; then
  chown --reference="$dst" "$tmp"
  chmod --reference="$dst" "$tmp"
else
  chown root:root "$tmp"
  chmod 0644 "$tmp"
fi
mv -f "$tmp" "$dst"
trap - EXIT
"#;

/// Runs `sudo` to write files as root.
#[derive(Debug, Clone)]
pub struct RootWriter {
    program: OsString,
    /// Where the text is staged; the temporary folder of the system when not set.
    staging: Option<PathBuf>,
}

impl Default for RootWriter {
    fn default() -> Self {
        Self::with_program("sudo")
    }
}

impl RootWriter {
    /// A writer that runs `program` instead of `sudo` (the tests bring their own).
    pub fn with_program(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            staging: None,
        }
    }

    /// Stages the text in `folder` instead of the temporary folder of the system.
    pub fn staging_in(mut self, folder: impl Into<PathBuf>) -> Self {
        self.staging = Some(folder.into());
        self
    }

    /// Writes `text` to `target` as root.
    pub fn write(&self, password: &Secret, target: &Path, text: &str) -> Result<(), RootError> {
        let staged = Staged::new(text, self.staging.as_deref())
            .map_err(|error| RootError::Failed(error.to_string()))?;
        // Through links, like the normal save.
        let target = std::fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());

        let mut child = self.spawn(staged.path(), &target)?;
        if let Some(mut stdin) = child.stdin.take() {
            // A closed pipe means sudo gave up already; what it said is in its output.
            let _ = stdin.write_all(password.bytes_for_sudo());
            let _ = stdin.write_all(b"\n");
        }
        let output = child
            .wait_with_output()
            .map_err(|error| RootError::Failed(error.to_string()))?;
        if output.status.success() {
            return Ok(());
        }
        Err(classify(&String::from_utf8_lossy(&output.stderr)))
    }
}

impl RootWriter {
    /// Starts `sudo` with the script. A program that was written a moment ago can
    /// be busy for a moment (another process that forked while it was open), so
    /// that is waited out a few times.
    fn spawn(&self, staged: &Path, target: &Path) -> Result<std::process::Child, RootError> {
        const BUSY: i32 = 26;
        let mut tries = 0;
        loop {
            let started = Command::new(&self.program)
                .args(["-S", "-k", "-p", "", "sh", "-c", SCRIPT, "sh"])
                .arg(staged)
                .arg(target)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn();
            match started {
                Ok(child) => return Ok(child),
                Err(error) if error.raw_os_error() == Some(BUSY) && tries < 20 => {
                    tries += 1;
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Err(RootError::NoSudo);
                }
                Err(error) => return Err(RootError::Failed(error.to_string())),
            }
        }
    }
}

impl Secret {
    fn bytes_for_sudo(&self) -> &[u8] {
        &self.bytes
    }
}

/// What `sudo` said, as an error.
pub fn classify(stderr: &str) -> RootError {
    let lower = stderr.to_lowercase();
    let first = stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("it stopped without saying why")
        .to_owned();
    let any = |needles: &[&str]| needles.iter().any(|needle| lower.contains(needle));
    if any(&[
        "incorrect password",
        "sorry, try again",
        "no password was provided",
        "a password is required",
    ]) {
        RootError::WrongPassword
    } else if any(&[
        "not in the sudoers",
        "is not allowed to",
        "may not run sudo",
    ]) {
        RootError::NotAllowed(first)
    } else {
        RootError::Failed(first)
    }
}

/// The new text in a file only the user can read, removed when this is dropped.
struct Staged {
    path: PathBuf,
}

impl Staged {
    fn new(text: &str, folder: Option<&Path>) -> std::io::Result<Self> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let folder = folder.map_or_else(std::env::temp_dir, Path::to_path_buf);
        let path = folder.join(format!("trcode-root-{}-{nanos}", std::process::id()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&path)?;
        let staged = Self { path };
        file.write_all(text.as_bytes())?;
        Ok(staged)
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
