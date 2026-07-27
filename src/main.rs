extern crate terminal_code;

use std::{
    env::{self, args_os},
    eprintln, panic,
    path::PathBuf,
    process::ExitCode,
};

use anyhow::{Result, bail};
use terminal_code::{App, StartupArgs, terminal};

fn main() -> ExitCode {
    install_panic_hook();

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            terminal::force_restore();
            eprintln!("Terminal Code Panic!!: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args = parse_args()?;

    let mut terminal = terminal::init()?;
    let mut app = App::new(args)?;

    let result = app.run(&mut terminal);
    terminal::restore(&mut terminal)?;
    result
}

fn install_panic_hook() {
    let default_hook = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {
        terminal::force_restore();
        default_hook(panic_info);
    }));
}

fn parse_args() -> Result<StartupArgs> {
    let mut args = args_os();
    _ = args.next();

    let path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or(env::current_dir()?);

    if args.next().is_some() {
        bail!("flag not yet impl")
    }

    Ok(StartupArgs::new(path))
}
