use std::io;
use std::time::Duration;

use crossterm::event::{Event as InputEvent, EventStream, KeyEvent};
use futures::StreamExt;
use ratatui::{Terminal, backend::Backend};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep_until};

use crate::action::Action;
use crate::clipboard::{Clipboard, Notice, Register};
use crate::components;
use crate::error::{IdeError, IdeResult};
use crate::event::Event;
use crate::keymap::{Context, Expiry, KeyChord, Keymap, Outcome, Resolver};
use crate::state::AppState;
use crate::terminal::KeyboardSupport;

/// Upper bound on redraw rate (about 60 fps).
const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const INPUT_CHANNEL_CAPACITY: usize = 256;
const EVENT_CHANNEL_CAPACITY: usize = 1024;
/// How long a half-typed key sequence waits for its next chord.
const SEQUENCE_TIMEOUT: Duration = Duration::from_millis(1000);
/// Contexts whose bindings apply to the focused editor, most specific first.
const EDITOR_CONTEXTS: [Context; 2] = [Context::Editor, Context::Global];

pub(crate) type InputResult = io::Result<InputEvent>;

/// Runs the app loop until the user quits or terminal input fails.
pub(crate) async fn run<B>(
    terminal: &mut Terminal<B>,
    mut sources: Sources,
    mut app: App,
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

        if app.needs_redraw() && Instant::now() >= draw_at {
            draw_frame(terminal, &mut app)?;
            last_draw = Some(Instant::now());
            continue;
        }

        // Biased: input is polled before bulk events so it is never starved.
        // The sleep only wakes the loop when a pending redraw becomes due; the
        // draw itself happens at the top of the loop, at most once per frame
        // interval.
        let sequence_deadline = app.pending_deadline().map(Instant::from_std);

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
            _ = sleep_until(sequence_deadline.unwrap_or_else(Instant::now)),
            if sequence_deadline.is_some() => app.expire_pending(),
        }

        if app.should_quit() {
            return Ok(());
        }
    }
}

/// The single owner of [`AppState`]. Everything else sends it events.
#[derive(Debug)]
pub(crate) struct App {
    flow: Flow,
    keymap: Keymap,
    state: AppState,
    resolver: Resolver,
    needs_redraw: bool,
    clipboard: Clipboard,
    sequence_timeout: Duration,
    pending_deadline: Option<std::time::Instant>,
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

    pub(crate) fn with_keymap(state: AppState, keymap: Keymap) -> Self {
        Self {
            state,
            keymap,
            clipboard: Clipboard::internal_only(),
            resolver: Resolver::default(),
            sequence_timeout: SEQUENCE_TIMEOUT,
            pending_deadline: None,
            flow: Flow::Continue,
            needs_redraw: true,
        }
    }

    /// Uses `clipboard` for copy, cut and paste. Without this the editor only
    /// has its own register.
    pub(crate) fn with_clipboard(mut self, clipboard: Clipboard) -> Self {
        self.clipboard = clipboard;
        self
    }

    #[cfg(test)]
    pub(crate) fn with_sequence_timeout(mut self, timeout: Duration) -> Self {
        self.sequence_timeout = timeout;
        self
    }

    /// When the half-typed key sequence, if any, gives up waiting.
    pub(crate) fn pending_deadline(&self) -> Option<std::time::Instant> {
        self.pending_deadline
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
        let resolution = self.resolver.feed(&self.keymap, &EDITOR_CONTEXTS, chord);

        for discarded in resolution.discarded {
            self.type_chord(discarded);
        }
        self.pending_deadline = None;
        match resolution.outcome {
            Outcome::Action(action) => self.run_action(action),
            Outcome::Pending => {
                self.pending_deadline = Some(std::time::Instant::now() + self.sequence_timeout);
            }
            Outcome::Unbound(chord) => self.type_chord(chord),
        }
    }

    /// Gives up on a half-typed key sequence: runs it if it is a binding by
    /// itself, otherwise types what can be typed.
    pub(crate) fn expire_pending(&mut self) {
        self.pending_deadline = None;
        match self.resolver.expire(&self.keymap, &EDITOR_CONTEXTS) {
            Expiry::Nothing => {}
            Expiry::Fire(action) => self.run_action(action),
            Expiry::Discard(chords) => chords.into_iter().for_each(|chord| self.type_chord(chord)),
        }
    }

    /// Types the chord if it is a printable key; other keys do nothing.
    fn type_chord(&mut self, chord: KeyChord) {
        if let Some(action) = typed_text(&chord) {
            self.run_action(action);
        }
    }

    fn run_action(&mut self, action: Action) {
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
            Action::DeleteWordBackward => self.state.editor_mut().delete_word_backward()?,
            Action::DeleteWordForward => self.state.editor_mut().delete_word_forward()?,
            Action::SelectAll => self.state.editor_mut().select_all()?,
            Action::Copy => self.copy()?,
            Action::Cut => self.cut()?,
            Action::Paste => self.paste()?,
            Action::Move(motion) => self.state.editor_mut().move_cursor(motion)?,
            Action::Select(motion) => self.state.editor_mut().select(motion)?,
            // Plugin actions need the plugin runtime, which is post-0.1.0.
            Action::Plugin(_) => {}
        }
        Ok(())
    }

    fn copy(&mut self) -> IdeResult {
        let register = self.state.editor().copy()?;
        self.clipboard.set(register);
        Ok(())
    }

    fn cut(&mut self) -> IdeResult {
        let register = self.state.editor_mut().cut()?;
        self.clipboard.set(register);
        Ok(())
    }

    fn paste(&mut self) -> IdeResult {
        let Some(fetched) = self.clipboard.get() else {
            self.state.set_status("nothing to paste");
            return Ok(());
        };
        self.state.editor_mut().paste(&fetched.register)?;
        if fetched.notice == Some(Notice::SystemUnreadable) {
            self.state
                .set_status("system clipboard is not readable here; pasted the editor's own copy");
        }
        Ok(())
    }

    /// Text the terminal pasted (bracketed paste, e.g. ctrl+shift+v or a middle
    /// click). Goes straight into the editor; it neither reads nor changes the
    /// clipboard.
    fn paste_from_terminal(&mut self, text: String) {
        self.state.clear_status();
        if let Err(error) = self.state.editor_mut().paste(&Register::charwise(text)) {
            self.state.set_status(error.to_string());
        }
        self.needs_redraw = true;
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
