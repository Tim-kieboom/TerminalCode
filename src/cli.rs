//! What the command line asks for: the version, the help, or files to open.

use std::ffi::OsString;
use std::path::PathBuf;

/// The name of the program on the command line (the `[[bin]]` in `Cargo.toml`).
const COMMAND: &str = "trcode";

/// What to do for the arguments the program was started with.
#[derive(Debug, PartialEq, Eq)]
pub enum Invocation {
    /// Start the editor with these files and folder.
    Open(Vec<PathBuf>),
    Help,
    Version,
}

impl Invocation {
    /// The first argument after the program name decides: `-h` or `--help`,
    /// `-v` or `--version`, otherwise every argument is a path. A file that is
    /// named like a flag is opened by putting `--` before it.
    pub fn from_args(args: impl Iterator<Item = OsString>) -> Self {
        let mut args = args.skip(1).peekable();
        match args.peek().and_then(|first| first.to_str()) {
            Some("-h" | "--help") => Self::Help,
            Some("-v" | "--version") => Self::Version,
            _ => Self::Open(crate::paths_from_args(
                std::iter::once(OsString::new()).chain(args),
            )),
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
         \n\
         Press f1 in the editor for the command palette.",
        version = env!("CARGO_PKG_VERSION"),
        description = env!("CARGO_PKG_DESCRIPTION"),
    )
}
