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
    let args = std::env::args_os();
    let (paths, note) = match Invocation::from_args(args)? {
        Invocation::Help => return print(help_text()),
        Invocation::Version => return print(version_text()),

        // A path named wins over `--recent`.
        Invocation::Open { paths, recent } if recent && paths.is_empty() => {
            let (folder, note) = terminal_code::recent_project();
            (folder.into_iter().collect(), note)
        }
        Invocation::Open { paths, .. } => (paths, None),
    };

    let environment = terminal_code::load_project_environment(&paths);
    let note = match (note, environment) {
        (Some(note), Some(environment)) => Some(format!("{note}; {environment}")),
        (note, environment) => note.or(environment),
    };

    set_restore_on_panic();
    let (terminal, capabilities) = terminal::init();
    let result = terminal_code::run(terminal, &paths, capabilities, note);
    terminal::restore();
    match result? {
        Some(folder) => relaunch_in(&folder),
        None => Ok(()),
    }
}

/// Starts this program again with `folder` as its project, in the same terminal. On
/// Unix the new program takes this one's place; elsewhere it runs as a child and
/// this one waits for it.
fn relaunch_in(folder: &std::path::Path) -> IdeResult {
    let mut command = std::process::Command::new(std::env::current_exe()?);
    // `--` so that a folder whose name starts with a dash is still a folder.
    command.arg("--").arg(folder);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(command.exec().into())
    }
    #[cfg(not(unix))]
    {
        command.status()?;
        Ok(())
    }
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
