use anyhow::Result;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{Stdout, stdout};

pub type AppTerminal = Terminal<CrosstermBackend<Stdout>>;
pub fn init() -> Result<AppTerminal> {
    enable_raw_mode()?;

    let mut output = stdout();
    execute!(output, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(output);
    let terminal = Terminal::new(backend)?;

    Ok(terminal)
}

pub fn restore(terminal: &mut AppTerminal) -> Result<()> {
    disable_raw_mode()?;

    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    terminal.show_cursor()?;
    Ok(())
}

pub fn force_restore() {
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), LeaveAlternateScreen);
}
