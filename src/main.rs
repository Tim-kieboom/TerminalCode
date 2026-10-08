use std::ffi::OsString;
use std::path::PathBuf;

use terminal_code::error::IdeResult;
use terminal_code::terminal;

fn main() {
    if let Err(err) = start() {
        eprint!("error: {err}")
    }
}

fn start() -> IdeResult {
    set_restore_on_panic();

    let path = path_from_args(std::env::args_os());
    let (terminal, capabilities) = terminal::init();
    let result = terminal_code::run(terminal, path.as_deref(), capabilities);
    terminal::restore();
    result
}

/// The file to open: the first argument after the program name. A leading
/// `--` marks the end of options and is skipped.
fn path_from_args(args: impl Iterator<Item = OsString>) -> Option<PathBuf> {
    let mut args = args.skip(1);
    let first = args.next()?;
    if first == "--" {
        return args.next().map(PathBuf::from);
    }
    Some(PathBuf::from(first))
}

fn set_restore_on_panic() {
    let original_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |info| {
        terminal::restore();
        original_hook(info);
    }));
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
