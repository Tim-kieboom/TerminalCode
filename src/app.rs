use std::io;
use std::time::Duration;

use crossterm::event::{Event as InputEvent, EventStream, KeyEvent};
use futures::StreamExt;
use ratatui::{Terminal, backend::Backend};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep_until};

use crate::action::Action;
use crate::error::{IdeError, IdeResult};
use crate::event::Event;
use crate::keymap::{KeyChord, Keymap};
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
    pub(crate) fn new(input: mpsc::Receiver<InputResult>, events: mpsc::Receiver<Event>) -> Self {
        Self { input, events }
    }

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
    keymap: Keymap,
    flow: Flow,
    needs_redraw: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::new(AppState::default())
    }
}

impl App {
    pub(crate) fn new(state: AppState) -> Self {
        Self {
            state,
            keymap: Keymap::default(),
            flow: Flow::Continue,
            needs_redraw: true,
        }
    }

    pub(crate) fn state(&self) -> &AppState {
        &self.state
    }

    pub(crate) fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    pub(crate) fn should_quit(&self) -> bool {
        self.flow == Flow::Quit
    }

    pub(crate) fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    pub(crate) fn handle_input(&mut self, input: InputEvent) {
        match input {
            InputEvent::Key(key) => self.handle_key(key),
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

    fn handle_key(&mut self, key: KeyEvent) {
        let Some(chord) = KeyChord::from_event(key) else {
            return;
        };
        let bound = self.keymap.lookup(&chord).cloned();
        let Some(action) = bound.or_else(|| typed_text(&chord)) else {
            return;
        };

        self.state.clear_status();
        if let Err(error) = self.dispatch(action) {
            self.state.set_status(error.to_string());
        }
        self.needs_redraw = true;
    }

    fn dispatch(&mut self, action: Action) -> IdeResult {
        match action {
            Action::Quit => self.flow = Flow::Quit,
            Action::Save => self.save()?,
            Action::Undo => self.state.editor_mut().undo()?,
            Action::Redo => self.state.editor_mut().redo()?,
            Action::InsertText(text) => self.state.editor_mut().insert_text(&text)?,
            Action::InsertNewline => self.state.editor_mut().insert_newline()?,
            Action::DeleteBackward => self.state.editor_mut().delete_backward()?,
            Action::DeleteForward => self.state.editor_mut().delete_forward()?,
            Action::Move(motion) => self.state.editor_mut().move_cursor(motion)?,
            Action::Select(motion) => self.state.editor_mut().select(motion)?,
            // Plugin actions need the plugin runtime, which is post-0.1.0.
            Action::Plugin(_) => {}
        }
        Ok(())
    }

    fn save(&mut self) -> IdeResult {
        self.state.editor_mut().save()?;
        let name = self.state.editor().display_name();
        self.state.set_status(format!("saved {name}"));
        Ok(())
    }

    pub(super) fn mark_drawn(&mut self) {
        self.needs_redraw = false;
    }
}

/// Text a key types when nothing is bound to it.
fn typed_text(chord: &KeyChord) -> Option<Action> {
    let typed = chord.typed_char()?;
    Some(Action::InsertText(typed.to_string().into()))
}

/// Runs the app loop until the user quits or terminal input fails.
pub(crate) async fn run<B>(
    terminal: &mut Terminal<B>,
    mut sources: Sources,
    state: AppState,
) -> IdeResult
where
    B: Backend,
    B::Error: Into<IdeError>,
{
    let mut app = App::new(state);
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
        .draw(|frame| ui::render(frame, app.state_mut()))
        .map_err(Into::into)?;

    app.mark_drawn();
    Ok(())
}
