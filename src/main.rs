extern crate terminal_code;

use std::{env::args_os, eprintln, panic, path::PathBuf, process::ExitCode};

use anyhow::{Result, bail};
use terminal_code::{App, StartupArgs, terminal};

fn main() -> ExitCode {
    install_panic_hook();

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            terminal::force_restore();
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args = parse_args()?;

    let mut terminal = terminal::init()?;
    let mut app = App::new(args);

    let result = app.run(&mut terminal);
    terminal::restore(&mut terminal)?;
    result
}

fn parse_args() -> Result<StartupArgs> {
    let mut args = args_os();
    let call_path = args
        .next()
        .ok_or(anyhow::anyhow!("args[0] execution_path not found"))?;

    let given_path = args.next().map(PathBuf::from);
    if args.next().is_some() {
        bail!("flag not yet impl")
    }

    let path = match given_path {
        Some(val) => val,
        None => {
            let mut path = PathBuf::from(call_path);
            path.pop();
            path
        }
    };

    Ok(StartupArgs::new(path))
}

fn install_panic_hook() {
    let default_hook = panic::take_hook();

    panic::set_hook(Box::new(move |panic_info| {
        terminal::force_restore();
        default_hook(panic_info);
    }));
}
