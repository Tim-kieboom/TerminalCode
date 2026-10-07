use std::io;
use std::time::Duration;

use crossterm::event::{
    Event as InputEvent, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use futures::StreamExt;
use ratatui::{Terminal, backend::Backend};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep_until};

use crate::error::{IdeError, IdeResult};
use crate::event::Event;
use crate::state::AppState;
use crate::ui;

/// Upper bound on redraw rate (about 60 fps).
const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const INPUT_CHANNEL_CAPACITY: usize = 256;
const EVENT_CHANNEL_CAPACITY: usize = 1024;

pub(crate) type InputResult = io::Result<InputEvent>;

/// Everything that can wake the app loop.
pub(crate) struct Sources {
    input: mpsc::Receiver<InputResult>,
    events: mpsc::Receiver<Event>,
}

impl Sources {
    /// Starts reading terminal input. The returned sender is how other tasks
    /// deliver [`Event`]s to the loop.
    pub(crate) fn spawn() -> (Self, mpsc::Sender<Event>) {
        let (input_tx, input) = mpsc::channel(INPUT_CHANNEL_CAPACITY);
        let (events_tx, events) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        tokio::spawn(read_terminal_input(input_tx));
        (Self { input, events }, events_tx)
    }
}

async fn read_terminal_input(tx: mpsc::Sender<InputResult>) {
    let mut stream = EventStream::new();
    while let Some(item) = stream.next().await {
        if tx.send(item).await.is_err() {
            return;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flow {
    Continue,
    Quit,
}

/// The single owner of [`AppState`]. Everything else sends it events.
#[derive(Debug)]
pub(crate) struct App {
    state: AppState,
    flow: Flow,
    needs_redraw: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            state: AppState::default(),
            flow: Flow::Continue,
            needs_redraw: true,
        }
    }
}

impl App {
    pub(crate) fn state(&self) -> &AppState {
        &self.state
    }

    pub(crate) fn should_quit(&self) -> bool {
        self.flow == Flow::Quit
    }

    pub(crate) fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    pub(crate) fn handle_input(&mut self, input: InputEvent) {
        match input {
            InputEvent::Key(key) if is_quit(key) => self.flow = Flow::Quit,
            InputEvent::Resize(..) => self.needs_redraw = true,
            _ => {}
        }
    }

    pub(crate) fn handle_event(&mut self, event: Event) {
        match event {
            Event::SetPluginView { id, content } => {
                self.state.set_plugin_view(id, content);
                self.needs_redraw = true;
            }
        }
    }

    fn mark_drawn(&mut self) {
        self.needs_redraw = false;
    }
}

/// Placeholder until the keymap lands in M2.
fn is_quit(key: KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
        && key.code == KeyCode::Char('q')
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

/// Runs the app loop until the user quits or terminal input fails.
pub(crate) async fn run<B>(terminal: &mut Terminal<B>, mut sources: Sources) -> IdeResult
where
    B: Backend,
    B::Error: Into<IdeError>,
{
    let mut app = App::default();
    let mut last_draw: Option<Instant> = None;

    loop {
        let draw_at = last_draw
            .map(|at| at + FRAME_INTERVAL)
            .unwrap_or(Instant::now());

        if app.needs_redraw() && Instant::now() >= draw_at {
            draw_frame(terminal, &mut app)?;
            last_draw = Some(Instant::now());
            continue;
        }

        // Biased: input is polled before bulk events so it is never starved.
        // The sleep only wakes the loop when a pending redraw becomes due; the
        // draw itself happens at the top of the loop, at most once per frame
        // interval.
        tokio::select! {
            biased;
            _ = sleep_until(draw_at),
            if app.needs_redraw() => {}
            input = sources.input.recv() => match input {
                Some(Ok(input)) => app.handle_input(input),
                Some(Err(err)) => return Err(err.into()),
                None => return Ok(()),
            },
            Some(event) = sources.events.recv() => app.handle_event(event),
        }

        if app.should_quit() {
            return Ok(());
        }
    }
}

fn draw_frame<B>(terminal: &mut Terminal<B>, app: &mut App) -> IdeResult
where
    B: Backend,
    B::Error: Into<IdeError>,
{
    terminal
        .draw(|frame| ui::render(frame, app.state()))
        .map_err(Into::into)?;

    app.mark_drawn();
    Ok(())
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
