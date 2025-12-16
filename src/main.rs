mod popups;
mod session;
mod text_editor;

use std::{env, io::stdout};
use anyhow::Result;
use crate::session::Session;


fn main() -> Result<()> {
    let base_path = env::current_exe()?;

    let mut session = Session::new(stdout(), base_path)?;
    session.run()
}
