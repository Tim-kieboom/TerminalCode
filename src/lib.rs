use crossterm::event::Event;
use ratatui::{Terminal, backend::Backend};

use crate::error::{IdeError, IdeResult};
use crate::state::AppState;

mod component;
pub mod error;
mod state;
mod ui;

pub fn run<B>(mut terminal: Terminal<B>) -> IdeResult
where
    B: Backend,
    B::Error: Into<IdeError>,
{
    let state = AppState::default();
    loop {
        terminal
            .draw(|frame| ui::render(frame, &state))
            .map_err(|e| e.into())?;

        let input = crossterm::event::read()?;
        if matches!(input, Event::Key(_)) {
            break Ok(());
        }
    }
}
