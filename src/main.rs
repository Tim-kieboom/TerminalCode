mod key_controller;
mod session;
mod window;

use crate::session::Session;
use anyhow::Result;
use std::{env, io::stdout};

fn main() -> Result<()> {
    let base_path = env::current_exe()?;

    let mut session = Session::new(stdout(), base_path)?;
    session.run()
}
