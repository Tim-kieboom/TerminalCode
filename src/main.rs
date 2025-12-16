mod session;
mod text_editor;

use std::{io::stdout, path::PathBuf, str::FromStr};
use anyhow::Result;
use crate::session::Session;


fn main() -> Result<()> {
    let mut session = Session::new(stdout())?;
    session.load_file(PathBuf::from_str("F:\\Code\\Github\\TerminalCode\\test.txt")?)?;
    session.run()
}
