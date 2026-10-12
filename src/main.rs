use std::process::ExitCode;

use terminal_code::error::IdeResult;
use terminal_code::terminal;
use terminal_code::{Invocation, help_text, version_text};

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
    // Help and version are answered before the terminal is touched.
    let paths = match Invocation::from_args(std::env::args_os()) {
        Invocation::Help => return print(help_text()),
        Invocation::Version => return print(version_text()),
        Invocation::Open(paths) => paths,
    };

    set_restore_on_panic();
    let (terminal, capabilities) = terminal::init();
    let result = terminal_code::run(terminal, &paths, capabilities);
    terminal::restore();
    result
}

fn print(text: String) -> IdeResult {
    println!("{text}");
    Ok(())
}

fn set_restore_on_panic() {
    let original_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |info| {
        terminal::restore();
        original_hook(info);
    }));
}
