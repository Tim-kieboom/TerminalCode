extern crate terminal_code;

use anyhow::Result;
use std::{env, io::stdout};
use terminal_code::Session;

fn main() -> Result<()> {
    let mut base_path = env::current_exe()?;
    if base_path.is_file() {
        base_path.pop();
    }

    let mut session = Session::new(stdout(), base_path)?;
    loop {
        match session.run() {
            Err(err) => session.display_error(err),
            Ok(()) => return session.dispose(),
        }
    }
}
