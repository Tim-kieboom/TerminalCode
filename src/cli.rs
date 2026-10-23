//! What the command line asks for: the version, the help, or files to open.

use std::ffi::OsString;
use std::path::PathBuf;

use thiserror::Error;

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

/// Why the command line was refused.
#[derive(Debug, Error)]
pub enum CliError {
    #[error("there can be no argument after `{0}`")]
    ArgumentAfterEndingFlag(Box<str>),
    #[error("unknown flag `{0}` (see --help; put -- before a file whose name starts with a dash)")]
    UnknownFlag(Box<str>),
}

impl Invocation {
    /// Reads the arguments after the program name, in any order: `-h`/`--help` and
    /// `-v`/`--version` must be the last argument, `-r`/`--recent` may be anywhere, and
    /// every other argument is a path. An argument that starts with a dash and is not a
    /// flag is refused; `--` ends the flags, so that a file named like a flag can still be
    /// opened.
    pub fn from_args(mut args: impl Iterator<Item = OsString>) -> Result<Self, CliError> {
        _ = args.next();

        let mut recent = false;
        let mut paths = Vec::new();
        while let Some(arg) = args.next() {
            // Only a flag has to be text: a path may be any bytes.
            match arg.to_str() {
                Some("--") => {
                    paths.extend(args.by_ref().map(PathBuf::from));
                }
                Some(flag @ ("-h" | "--help")) => return ending(flag, Self::Help, args.next()),
                Some(flag @ ("-v" | "--version")) => {
                    return ending(flag, Self::Version, args.next());
                }
                Some("-r" | "--recent") => recent = true,
                Some(flag) if flag.starts_with('-') => {
                    return Err(CliError::UnknownFlag(flag.into()));
                }
                _ => paths.push(PathBuf::from(arg)),
            }
        }

        Ok(Self::Open { paths, recent })
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
         --help and --version must be the last argument.\n\
         \n\
         Options:\n\
         \x20 -h, --help     Print this help\n\
         \x20 -v, --version  Print the version\n\
         \x20 -r, --recent   Open the newest recent project folder that is still there\n\
         \x20                (the current folder when none is); a path named wins\n\
         \n\
         Press f1 in the editor for the command palette.",
        version = env!("CARGO_PKG_VERSION"),
        description = env!("CARGO_PKG_DESCRIPTION"),
    )
}

/// `invocation` for a flag that must come last: `next` is what came after it.
fn ending(
    flag: &str,
    invocation: Invocation,
    next: Option<OsString>,
) -> Result<Invocation, CliError> {
    match next {
        Some(_) => Err(CliError::ArgumentAfterEndingFlag(flag.into())),
        None => Ok(invocation),
    }
}
