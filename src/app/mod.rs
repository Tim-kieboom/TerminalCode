//! The app: the single owner of the state, its event loop, and what each kind
//! of input does. The loop and the plumbing are here; what keys, the mouse,
//! the popups and outside changes do is in the modules below.

use std::io;
use std::time::Duration;

use crossterm::event::{Event as InputEvent, EventStream};
use futures::StreamExt;
use ratatui::{Terminal, backend::Backend};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep_until};

use crate::app::state::{AppState, Popup};
use crate::clipboard::Clipboard;
use crate::components;
use crate::error::{IdeError, IdeResult};
use crate::event::Event;
use crate::keymap::Keymap;
use crate::terminal::{self, KeyboardSupport};

mod background;
mod editing;
mod keys;
mod mouse;
mod panels;
mod popups;
pub(crate) mod state;
mod watching;

use background::Background;
use keys::Keyboard;
use mouse::MouseInput;
use popups::{Remembered, Removal};
use watching::Watching;

/// Upper bound on redraw rate (about 60 fps).
const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const INPUT_CHANNEL_CAPACITY: usize = 256;
const EVENT_CHANNEL_CAPACITY: usize = 1024;

pub(crate) type InputResult = io::Result<InputEvent>;

/// The single owner of [`AppState`]. Everything else sends it events.
#[derive(Debug)]
pub(crate) struct App {
    state: AppState,
    /// Whether the loop is to go on or stop.
    flow: Flow,
    needs_redraw: bool,
    keyboard: Keyboard,
    mouse: MouseInput,
    clipboard: Clipboard,
    watching: Watching,
    background: Background,
    remembered: Remembered,
    removal: Removal,
}

/// Everything that can wake the app loop.
pub(crate) struct Sources {
    pub(super) input: mpsc::Receiver<InputResult>,
    pub(super) events: mpsc::Receiver<Event>,
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

impl Default for App {
    fn default() -> Self {
        Self::new(AppState::default())
    }
}

impl App {
    pub(crate) fn new(state: AppState) -> Self {
        Self::with_keymap(state, Keymap::defaults(KeyboardSupport::Enhanced))
    }

    /// Runs the app loop until the user quits or terminal input fails.
    pub(crate) async fn run<B>(
        mut self,
        terminal: &mut Terminal<B>,
        mut sources: Sources,
    ) -> IdeResult
    where
        B: Backend,
        B::Error: Into<IdeError>,
    {
        let mut last_draw: Option<Instant> = None;

        loop {
            let draw_at = last_draw
                .map(|at| at + FRAME_INTERVAL)
                .unwrap_or(Instant::now());

            if self.needs_redraw() && Instant::now() >= draw_at {
                draw_frame(terminal, &mut self)?;
                last_draw = Some(Instant::now());
                continue;
            }

            // Biased: input is polled before bulk events so it is never starved.
            // The sleep only wakes the loop when a pending redraw becomes due; the
            // draw itself happens at the top of the loop, at most once per frame
            // interval.
            let sequence_deadline = self.pending_deadline().map(Instant::from_std);
            let notification_deadline = self.notification_deadline().map(Instant::from_std);

            tokio::select! {
                biased;
                _ = sleep_until(draw_at),
                if self.needs_redraw() => {}
                input = sources.input.recv() => match input {
                    Some(Ok(input)) => self.handle_input(input),
                    Some(Err(err)) => return Err(err.into()),
                    None => return Ok(()),
                },
                Some(event) = sources.events.recv() => self.handle_event(event),
                _ = sleep_until(sequence_deadline.unwrap_or_else(Instant::now)),
                if sequence_deadline.is_some() => self.expire_pending(),
                _ = sleep_until(notification_deadline.unwrap_or_else(Instant::now)),
                if notification_deadline.is_some() => self.expire_notifications(std::time::Instant::now()),
            }

            self.sync_watches();
            if let Some(enabled) = self.take_mouse_change() {
                terminal::set_mouse_capture(enabled);
            }

            if self.should_quit() {
                return Ok(());
            }
        }
    }

    pub(crate) fn with_keymap(state: AppState, keymap: Keymap) -> Self {
        Self {
            state,
            flow: Flow::Continue,
            needs_redraw: true,
            keyboard: Keyboard::new(keymap),
            mouse: MouseInput::default(),
            clipboard: Clipboard::internal_only(),
            watching: Watching::default(),
            background: Background::default(),
            remembered: Remembered::default(),
            removal: Removal::default(),
        }
    }

    /// Uses `clipboard` for copy, cut and paste. Without this the editor only
    /// has its own register.
    pub(crate) fn with_clipboard(mut self, clipboard: Clipboard) -> Self {
        self.clipboard = clipboard;
        self
    }

    /// Gives the app a way to start background work that reports back through
    /// the event channel.
    pub(crate) fn with_events(mut self, events: mpsc::Sender<Event>) -> Self {
        self.background.connect(events);
        self
    }

    /// Moves files to `trash` instead of the OS trash.
    #[cfg(test)]
    pub(crate) fn with_trash(mut self, trash: crate::removal::TrashFn) -> Self {
        self.removal = Removal::with_trash(trash);
        self
    }

    #[cfg(test)]
    pub(crate) fn with_sequence_timeout(mut self, timeout: Duration) -> Self {
        self.keyboard = self.keyboard.with_sequence_timeout(timeout);
        self
    }

    /// When the next info message goes away, if there is one.
    pub(crate) fn notification_deadline(&self) -> Option<std::time::Instant> {
        self.state.notifications().next_expiry()
    }

    /// Removes the info messages that have run out.
    pub(crate) fn expire_notifications(&mut self, now: std::time::Instant) {
        if self.state.expire_notifications(now) {
            self.needs_redraw = true;
        }
    }

    /// When the half-typed key sequence, if any, gives up waiting.
    pub(crate) fn pending_deadline(&self) -> Option<std::time::Instant> {
        self.keyboard.pending_deadline()
    }

    #[cfg(test)]
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
            InputEvent::Paste(text) => self.paste_from_terminal(text),
            InputEvent::Mouse(event) => self.handle_mouse(event),
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
            Event::FilesChanged(paths) => self.files_changed(&paths),
            Event::FinderBatch { scan, files } => {
                if let Some(finder) = self.state.finder_mut() {
                    finder.add_batch(scan, files);
                    self.needs_redraw = true;
                }
            }
            Event::FinderDone { scan, unreadable } => {
                if let Some(finder) = self.state.finder_mut() {
                    finder.finish(scan, unreadable);
                    self.needs_redraw = true;
                }
            }
            Event::SearchBatch { search, hits } => {
                if let Some(open) = self.state.search_mut() {
                    open.add_hits(search, hits);
                    self.needs_redraw = true;
                }
            }
            Event::SearchDone {
                search,
                files,
                truncated,
            } => {
                if let Some(open) = self.state.search_mut() {
                    open.finish(search, files, truncated);
                    self.needs_redraw = true;
                }
            }
            Event::WatchFailed(message) => {
                self.state.notify_error(format!("file watcher: {message}"));
                self.needs_redraw = true;
            }
        }
    }

    /// Whether an overlay is open and taking all input.
    fn modal_open(&self) -> bool {
        !matches!(self.state.popup(), Popup::None)
    }

    pub(super) fn mark_drawn(&mut self) {
        self.needs_redraw = false;
    }
}

fn draw_frame<B>(terminal: &mut Terminal<B>, app: &mut App) -> IdeResult
where
    B: Backend,
    B::Error: Into<IdeError>,
{
    terminal
        .draw(|frame| components::prepare_and_render(frame, app.state_mut()))
        .map_err(Into::into)?;

    app.mark_drawn();
    Ok(())
}
