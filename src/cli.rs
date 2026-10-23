//! What the command line asks for: the version, the help, or files to open.

use std::ffi::OsString;
use std::path::PathBuf;

/// The name of the program on the command line (the `[[bin]]` in `Cargo.toml`).
const COMMAND: &str = "trcode";

/// What to do for the arguments the program was started with.
#[derive(Debug, PartialEq, Eq)]
pub enum Invocation {
    /// Start the editor with these files and folder; `recent` is `--recent`: with no
    /// path named, the project is the newest recent folder that is still there.
    Open {
        paths: Vec<PathBuf>,
        recent: bool,
    },
    Help,
    Version,
}

impl Invocation {
    /// `--recent` first, if there; then the first argument decides: `-h` or
    /// `--help`, `-v` or `--version`, otherwise every argument is a path. A file
    /// that is named like a flag is opened by putting `--` before it.
    pub fn from_args(args: impl Iterator<Item = OsString>) -> Self {
        let mut args = args.skip(1).peekable();
        let mut recent = false;
        while args.peek().is_some_and(|first| first == "--recent") {
            recent = true;
            args.next();
        }
        match args.peek().and_then(|first| first.to_str()) {
            Some("-h" | "--help") => Self::Help,
            Some("-v" | "--version") => Self::Version,
            _ => Self::Open {
                paths: crate::paths_from_args(std::iter::once(OsString::new()).chain(args)),
                recent,
            },
        }
    }
}

/// What `--version` prints.
pub fn version_text() -> String {
    format!("{} {}", COMMAND, env!("CARGO_PKG_VERSION"))
}

/// What `--help` prints.
pub fn help_text() -> String {
    let name = COMMAND;
    format!(
        "{name} {version}\n\
         {description}\n\
         \n\
         Usage: {name} [OPTIONS] [folder] [file ...]\n\
         \n\
         The first folder is the project (the current folder when none is named);\n\
         every file opens in a tab. Put -- before a file whose name starts with a dash.\n\
         \n\
         Options:\n\
         \x20 -h, --help     Print this help\n\
         \x20 -v, --version  Print the version\n\
         \x20     --recent   Open the newest recent project folder that is still there\n\
         \x20                (the current folder when none is); a path named wins\n\
         \n\
         Press f1 in the editor for the command palette.",
        version = env!("CARGO_PKG_VERSION"),
        description = env!("CARGO_PKG_DESCRIPTION"),
    )
}
