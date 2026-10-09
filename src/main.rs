use std::path::PathBuf;
use std::process::ExitCode;

use terminal_code::error::IdeResult;
use terminal_code::terminal;

fn main() -> ExitCode {
    match start() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn start() -> IdeResult {
    set_restore_on_panic();

    let paths = parse_args();
    let (terminal, capabilities) = terminal::init();
    let result = terminal_code::run(terminal, &paths, capabilities);
    terminal::restore();
    result
}

fn parse_args() -> Vec<PathBuf> {
    let args = std::env::args_os();
    terminal_code::paths_from_args(args)
}

fn set_restore_on_panic() {
    let original_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |info| {
        terminal::restore();
        original_hook(info);
    }));
}
