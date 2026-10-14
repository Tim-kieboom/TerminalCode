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
use crate::config::ThemeChoice;
use crate::error::{IdeError, IdeResult};
use crate::event::Event;
use crate::keymap::Keymap;
use crate::pty::Shell;
use crate::settings::Settings;
use crate::syntax::SyntaxWorker;
use crate::terminal::{self, KeyboardSupport};

mod background;
mod bars;
mod completion;
mod config_files;
mod definition;
mod editing;
mod errors_view;
mod hover;
mod open;
mod recent;
#[cfg(test)]
pub(crate) use open::expand_path;
mod jumps;
mod keys;
mod lsp;
mod mouse;
mod panels;
mod popups;
mod reset;
mod saved;
pub(crate) mod state;
mod terminal_keys;
mod terminal_pane;
mod watching;

use background::Background;
use keys::Keyboard;
use mouse::MouseInput;
use popups::{Remembered, Removal};
use saved::Saved;
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
    /// The folder the program starts again in once the editor ends, when Open Folder
    /// asked for it.
    restart_in: Option<std::path::PathBuf>,
    needs_redraw: bool,
    keyboard: Keyboard,
    mouse: MouseInput,
    /// Where the cursor has been, for going back.
    jumps: jumps::Jumps,
    clipboard: Clipboard,
    watching: Watching,
    background: Background,
    remembered: Remembered,
    removal: Removal,
    /// What saving as root needs: the password prompt's state and how to run sudo.
    root: popups::Root,
    /// What the terminal pane runs.
    terminal_shell: Shell,
    /// What runs when `terminal_shell` cannot start: the platform's own.
    fallback_shell: Shell,
    /// When `ctrl+b` in the terminal gives up waiting for the next key.
    terminal_prefix: Option<std::time::Instant>,
    /// The directory of the user's files; `None` when there is none, in which
    /// case the actions that open them say so.
    config_dir: Option<std::path::PathBuf>,
    /// What the terminal can do with keys, which decides the keymap's fallbacks.
    keyboard_support: KeyboardSupport,
    /// The theme in use: a built-in one or an installed one.
    theme_choice: ThemeChoice,
    /// The settings last applied; a reload applies a setting only when the file changed it.
    settings: Settings,
    /// What the config files said when they were last applied.
    config_applied: Option<u64>,
    /// The tab that was in front when the last input was handled, to notice a
    /// switch to another.
    front_view: Option<crate::components::workspace::ViewId>,
    /// The panes the user hid or showed, remembered for the next run.
    saved: Saved,
    recent: recent::Recents,
    /// The language servers.
    lsp: lsp::Lsp,
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
    ) -> IdeResult<Option<std::path::PathBuf>>
    where
        B: Backend,
        B::Error: Into<IdeError>,
    {
        let mut last_draw: Option<Instant> = None;

        // A setting may have switched the mouse off before the first frame.
        if let Some(enabled) = self.take_mouse_change() {
            terminal::set_mouse_capture(enabled);
        }
        // The file named on the command line is in front before any input.
        self.lsp_file_shown();

        loop {
            let draw_at = last_draw
                .map(|at| at + FRAME_INTERVAL)
                .unwrap_or(Instant::now());

            if self.needs_redraw() && Instant::now() >= draw_at {
                // What was typed since the last frame is one message to the servers.
                self.lsp_sync();
                self.completion_step();
                self.hover_step();
                self.hover_forget_stale();
                self.definition_forget_stale();
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
            let hover_deadline = self.hover_deadline().map(Instant::from_std);

            tokio::select! {
                biased;
                _ = sleep_until(draw_at),
                if self.needs_redraw() => {}
                input = sources.input.recv() => match input {
                    Some(Ok(input)) => self.handle_input(input),
                    Some(Err(err)) => return Err(err.into()),
                    None => return Ok(None),
                },
                Some(event) = sources.events.recv() => self.handle_event(event),
                _ = sleep_until(sequence_deadline.unwrap_or_else(Instant::now)),
                if sequence_deadline.is_some() => self.expire_pending(),
                _ = sleep_until(notification_deadline.unwrap_or_else(Instant::now)),
                if notification_deadline.is_some() => self.expire_notifications(std::time::Instant::now()),
                _ = sleep_until(hover_deadline.unwrap_or_else(Instant::now)),
                if hover_deadline.is_some() => self.hover_due(),
            }

            self.sync_watches();
            if let Some(enabled) = self.take_mouse_change() {
                terminal::set_mouse_capture(enabled);
            }

            if self.should_quit() {
                return Ok(self.restart_in.take());
            }
        }
    }

    pub(crate) fn with_keymap(state: AppState, keymap: Keymap) -> Self {
        Self {
            state,
            flow: Flow::Continue,
            restart_in: None,
            needs_redraw: true,
            keyboard: Keyboard::new(keymap),
            mouse: MouseInput::default(),
            jumps: jumps::Jumps::default(),
            clipboard: Clipboard::internal_only(),
            watching: Watching::default(),
            background: Background::default(),
            remembered: Remembered::default(),
            removal: Removal::default(),
            root: popups::Root::default(),
            terminal_shell: Shell::detect(),
            fallback_shell: Shell::from_setting(None),
            terminal_prefix: None,
            config_dir: None,
            keyboard_support: KeyboardSupport::Enhanced,
            theme_choice: ThemeChoice::default(),
            settings: Settings::default(),
            config_applied: None,
            front_view: None,
            lsp: lsp::Lsp::default(),
            saved: Saved::new([true, false, true, true]),
            recent: recent::Recents::default(),
        }
    }

    /// Tells a reload which keymap fallbacks the terminal needs.
    pub(crate) fn with_keyboard_support(mut self, support: KeyboardSupport) -> Self {
        self.keyboard_support = support;
        self
    }

    /// Where `Action::OpenConfig` finds and makes the user's files.
    pub(crate) fn with_config_dir(mut self, dir: Option<std::path::PathBuf>) -> Self {
        self.config_dir = dir;
        self.config_applied = self.config_hash();
        self
    }

    /// Starts with `settings` applied, `theme` being the theme they name.
    pub(crate) fn with_settings(mut self, settings: Settings, theme: ThemeChoice) -> Self {
        let before = std::mem::replace(&mut self.settings, settings);
        self.apply_runtime_settings(&before);
        self.theme_choice = theme;
        self.config_applied = self.config_hash();
        self
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
        self.start_syntax_worker();
        self
    }

    /// Starts a syntax worker with the current theme, in place of the one there
    /// is, and has every document ask it for its colors. Nothing happens without
    /// a way to hear back from it.
    fn start_syntax_worker(&mut self) {
        let Some(results) = self.background.sender() else {
            return;
        };
        let theme = self.state.theme_for_worker();
        self.state
            .set_syntax_worker(SyntaxWorker::spawn(theme, move |output| {
                let _ = results.blocking_send(Event::Highlighted(output));
            }));
        self.state.workspace_mut().restart_highlights();
        self.needs_redraw = true;
    }

    /// Runs `shell` in the terminal pane instead of the user's.
    #[cfg(test)]
    pub(crate) fn with_terminal_shell(mut self, shell: Shell) -> Self {
        self.terminal_shell = shell;
        self
    }

    /// The program the next terminal tab would run.
    #[cfg(test)]
    pub(crate) fn terminal_shell(&self) -> &Shell {
        &self.terminal_shell
    }

    /// Falls back to `shell` instead of the platform's when the terminal's cannot start.
    #[cfg(test)]
    pub(crate) fn with_fallback_shell(mut self, shell: Shell) -> Self {
        self.fallback_shell = shell;
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
        [
            self.keyboard.pending_deadline(),
            self.terminal_prefix_deadline(),
        ]
        .into_iter()
        .flatten()
        .min()
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
        let before = self.here();
        match &input {
            InputEvent::Key(key) => self.handle_key(*key),
            InputEvent::Paste(text) => self.paste_from_terminal(text.clone()),
            InputEvent::Mouse(event) => self.handle_mouse(*event),
            InputEvent::Resize(..) => self.needs_redraw = true,
            // Back from another window: whatever was open may have changed.
            InputEvent::FocusGained => self.check_files_on_disk(),
            _ => {}
        }
        self.track_jump(&input, before);
        self.notice_tab_switch();
        self.show_pending_keys();
        self.remember_panes();
        self.remember_recent();
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
            Event::TerminalChanged => self.terminal_changed(),
            Event::LspChanged => self.lsp_changed(),
            Event::RootSaved(result) => self.root_save_finished(result),
            Event::Highlighted(output) => {
                if self.state.workspace_mut().accept_highlights(output) {
                    self.needs_redraw = true;
                }
            }
            Event::WatchFailed(message) => {
                self.state.notify_error(format!("file watcher: {message}"));
                self.needs_redraw = true;
            }
        }
        self.remember_panes();
        self.remember_recent();
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
