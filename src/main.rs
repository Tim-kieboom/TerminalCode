mod key_controller;
mod context;
mod session;
mod window;

use crate::session::Session;
use anyhow::Result;
use std::{env, io::stdout};

fn main() -> Result<()> {
    let mut base_path = env::current_exe()?;
    if base_path.is_file() {
        base_path.pop();
    }

    let mut session = Session::new(stdout(), base_path)?;
    session.run()
}
