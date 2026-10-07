use ratatui::{Terminal, backend::Backend};

use crate::app::Sources;
use crate::error::{IdeError, IdeResult};

mod app;
mod buffer;
mod component;
pub mod error;
mod event;
mod state;
mod ui;

pub fn run<B>(mut terminal: Terminal<B>) -> IdeResult
where
    B: Backend,
    B::Error: Into<IdeError>,
{
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        let (sources, _events) = Sources::spawn();
        app::run(&mut terminal, sources).await
    })
}
