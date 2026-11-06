//! Opening a file with the system's program for it, as a double click would.

use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

/// The program that opens files on this system, and what goes before the path.
pub(crate) fn system_opener() -> (&'static str, &'static [&'static str]) {
    if cfg!(target_os = "windows") {
        // Not `cmd /c start`: that is a shell, and a path with `&` in it would run as two
        // commands.
        ("rundll32", &["url.dll,FileProtocolHandler"])
    } else if cfg!(target_os = "macos") {
        ("open", &[])
    } else {
        ("xdg-open", &[])
    }
}

/// The arguments for `path`. The path is whole (not from the folder the opener starts in)
/// so that a name that starts with `-` is never taken for an option.
pub(crate) fn arguments(front: &[&str], path: &Path) -> Vec<OsString> {
    let mut arguments: Vec<OsString> = front.iter().map(OsString::from).collect();
    arguments.push(path.as_os_str().to_owned());
    arguments
}

/// Starts `program` to open `path` and does not wait for it: that it started is all that is
/// asked. It has no terminal, no input and no output. Its end is waited for on a thread, so
/// it does not stay a zombie.
pub(crate) fn open(program: &str, front: &[&str], path: &Path) -> io::Result<()> {
    let mut child = Command::new(program)
        .args(arguments(front, path))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Starts `program` with `args`, in `directory`, and does not wait for it: the terminal
/// program or file manager is a window of its own. It has no input and no output. Its end
/// is waited for on a thread, so it does not stay a zombie.
pub(crate) fn start(program: &str, args: &[String], directory: &Path) -> io::Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests;
