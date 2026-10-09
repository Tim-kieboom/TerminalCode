use std::io;
use std::time::Duration;

use crossterm::event::{
    Event as InputEvent, EventStream, KeyEvent, KeyEventKind, KeyModifiers, MouseButton,
    MouseEvent, MouseEventKind,
};
use futures::StreamExt;
use ratatui::{Terminal, backend::Backend};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep_until};

use crate::clipboard::{Clipboard, Notice, Register};
use crate::components::explorer::ExplorerCommand;
use crate::components::finder::{self, Finder};
use crate::components::palette::{self, Entry, Palette};
use crate::components::quit_prompt::{self, QuitPrompt};
use crate::components::workspace::{CloseResult, DiskEvent, DocumentId, SaveOutcome, canonical};
use crate::components::{self, ComponentKind};
use crate::error::{IdeError, IdeResult};
use crate::event::{Event, action::Action, mouse::ClickTracker};
use crate::keymap::{Context, Expiry, KeyChord, Keymap, Outcome, Resolver};
use crate::state::{AppState, Focus};
use crate::terminal::{self, KeyboardSupport};
use crate::ui::layout::Axis;
use crate::watcher::FsWatcher;

/// Upper bound on redraw rate (about 60 fps).
const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const INPUT_CHANNEL_CAPACITY: usize = 256;
const EVENT_CHANNEL_CAPACITY: usize = 1024;
/// How long a half-typed key sequence waits for its next chord.
const SEQUENCE_TIMEOUT: Duration = Duration::from_millis(1000);
/// Contexts whose bindings apply to the focused editor, most specific first.
/// Shown when an editing action arrives while every tab is closed.
const NO_FILE_MESSAGE: &str = "no open file (ctrl+n opens a new one)";
const EDITOR_CONTEXTS: [Context; 2] = [Context::Editor, Context::Global];
const EXPLORER_CONTEXTS: [Context; 2] = [Context::Explorer, Context::Global];
/// Where the palette looks for the keys shown beside an action.
const PALETTE_CONTEXTS: [Context; 3] = [Context::Explorer, Context::Editor, Context::Global];

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
        let notification_deadline = app.notification_deadline().map(Instant::from_std);

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
            _ = sleep_until(notification_deadline.unwrap_or_else(Instant::now)),
            if notification_deadline.is_some() => app.expire_notifications(std::time::Instant::now()),
        }

        app.sync_watches();
        if let Some(enabled) = app.take_mouse_change() {
            terminal::set_mouse_capture(enabled);
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
    clicks: ClickTracker,
    mouse_enabled: bool,
    mouse_change: Option<bool>,
    sequence_timeout: Duration,
    pending_deadline: Option<std::time::Instant>,
    watcher: Option<FsWatcher>,
    /// The explorer and document versions the watches were last set for.
    watched_versions: Option<(u64, u64)>,
    /// Whether a watch failure was already reported, so it is shown once.
    watch_failure_shown: bool,
    /// Where background work sends its results; without it the file finder
    /// has no way to get its files.
    events: Option<mpsc::Sender<Event>>,
    /// Numbers the file finder's walks so a stale walk's files are ignored.
    scans_started: u64,
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
            clicks: ClickTracker::default(),
            mouse_enabled: true,
            mouse_change: None,
            resolver: Resolver::default(),
            sequence_timeout: SEQUENCE_TIMEOUT,
            pending_deadline: None,
            watcher: None,
            watched_versions: None,
            watch_failure_shown: false,
            events: None,
            scans_started: 0,
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

    /// Gives the app a way to start background work that reports back through
    /// the event channel.
    pub(crate) fn with_events(mut self, events: mpsc::Sender<Event>) -> Self {
        self.events = Some(events);
        self
    }

    /// Watches the directories of open files and of the explorer for changes
    /// made outside the editor.
    pub(crate) fn with_watcher(mut self, watcher: FsWatcher) -> Self {
        self.watcher = Some(watcher);
        self.sync_watches();
        self
    }

    #[cfg(test)]
    pub(crate) fn watched_directories(&self) -> Vec<std::path::PathBuf> {
        self.watcher
            .as_ref()
            .map(|watcher| watcher.watched().iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Makes the watched directories match what is open. Cheap when nothing
    /// was opened or closed since the last call.
    pub(crate) fn sync_watches(&mut self) {
        let Some(watcher) = self.watcher.as_mut() else {
            return;
        };
        let versions = (
            self.state.explorer().version(),
            self.state.workspace().documents_version(),
        );
        if self.watched_versions == Some(versions) {
            return;
        }
        self.watched_versions = Some(versions);

        let wanted: std::collections::BTreeSet<_> = self
            .state
            .explorer()
            .open_directories()
            .iter()
            .map(|dir| canonical(dir))
            .chain(self.state.workspace().directories())
            .collect();
        if let Err(error) = watcher.watch_only(&wanted)
            && !self.watch_failure_shown
        {
            self.watch_failure_shown = true;
            self.state.notify_error(format!(
                "changes made outside the editor may be missed: {error}"
            ));
            self.needs_redraw = true;
        }
    }

    #[cfg(test)]
    pub(crate) fn with_sequence_timeout(mut self, timeout: Duration) -> Self {
        self.sequence_timeout = timeout;
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
            Event::WatchFailed(message) => {
                self.state.notify_error(format!("file watcher: {message}"));
                self.needs_redraw = true;
            }
        }
    }

    /// Files changed outside the editor: brings open documents in step (see
    /// [`crate::components::workspace::Workspace::check_disk`]) and refreshes
    /// the explorer.
    fn files_changed(&mut self, paths: &[std::path::PathBuf]) {
        for event in self.state.workspace_mut().check_disk(paths) {
            match event {
                DiskEvent::Reloaded(name) => {
                    self.state
                        .notify(format!("{name} changed on disk; reloaded it"));
                }
                DiskEvent::Conflict(name) => self.state.notify_error(format!(
                    "{name} changed on disk; your changes are kept (saving asks before overwriting)"
                )),
                DiskEvent::Deleted(name) => {
                    self.state.notify(format!("{name} was deleted on disk"));
                }
                DiskEvent::Unreadable(message) => self.state.notify_error(message),
            }
        }
        self.refresh_explorer_for(paths);
        self.needs_redraw = true;
    }

    /// Re-reads the explorer's directories if any of `paths` is inside the
    /// project.
    fn refresh_explorer_for(&mut self, paths: &[std::path::PathBuf]) {
        let Some(root) = self.state.explorer().root_path().map(canonical) else {
            return;
        };
        if !paths.iter().any(|path| canonical(path).starts_with(&root)) {
            return;
        }
        if let Err(error) = self.state.explorer_mut().refresh() {
            self.state.notify_error(error.to_string());
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        // An error has had its moment once the user acts. Dismissed before the key
        // is handled, so an error that this very key causes stays, and the key
        // itself still does its job.
        if key.kind == KeyEventKind::Press && self.state.dismiss_errors() {
            self.needs_redraw = true;
        }
        if self.state.palette().is_some() {
            self.handle_palette_key(key);
            return;
        }
        if self.state.finder().is_some() {
            self.handle_finder_key(key);
            return;
        }
        if self.state.quit_prompt().is_some() {
            self.handle_prompt_key(key);
            return;
        }
        let Some(chord) = KeyChord::from_event(key) else {
            return;
        };
        let contexts = self.contexts();
        let resolution = self.resolver.feed(&self.keymap, contexts, chord);

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
        let contexts = self.contexts();
        match self.resolver.expire(&self.keymap, contexts) {
            Expiry::Nothing => {}
            Expiry::Fire(action) => self.run_action(action),
            Expiry::Discard(chords) => chords.into_iter().for_each(|chord| self.type_chord(chord)),
        }
    }

    /// The contexts whose bindings apply to the component with the keyboard.
    fn contexts(&self) -> &'static [Context] {
        match self.state.focus() {
            Focus::Editor => &EDITOR_CONTEXTS,
            Focus::Explorer => &EXPLORER_CONTEXTS,
        }
    }

    /// Types the chord if it is a printable key and the editor has the
    /// keyboard; other keys do nothing.
    fn type_chord(&mut self, chord: KeyChord) {
        if self.state.focus() != Focus::Editor {
            return;
        }
        if let Some(action) = typed_text(&chord) {
            self.run_action(action);
        }
    }

    fn run_action(&mut self, action: Action) {
        if let Err(error) = self.dispatch(action) {
            self.state.notify_error(error.to_string());
        }
        self.needs_redraw = true;
    }

    fn dispatch(&mut self, action: Action) -> IdeResult {
        if action.needs_editor() && !self.state.workspace().has_tabs() {
            self.state.notify(NO_FILE_MESSAGE);
            return Ok(());
        }
        if action.focuses_editor() {
            self.state.set_focus(Focus::Editor);
        }
        match action {
            Action::Quit => self.quit(),
            Action::CommandPalette => self.open_palette(),
            Action::FindFile => self.open_finder(),
            Action::FocusExplorer => self.toggle_explorer_focus(),
            Action::Explorer(command) => self.explorer_command(command)?,
            Action::Save => self.save()?,
            Action::Undo => self.state.edit(|editor| editor.undo())?,
            Action::Redo => self.state.edit(|editor| editor.redo())?,
            Action::InsertText(text) => self.state.edit(|editor| editor.insert_text(&text))?,
            Action::InsertNewline => self.state.edit(|editor| editor.insert_newline())?,
            Action::DeleteBackward => self.state.edit(|editor| editor.delete_backward())?,
            Action::DeleteForward => self.state.edit(|editor| editor.delete_forward())?,
            Action::DeleteWordBackward => {
                self.state.edit(|editor| editor.delete_word_backward())?
            }
            Action::DeleteWordForward => self.state.edit(|editor| editor.delete_word_forward())?,
            Action::SelectAll => self.state.edit(|editor| editor.select_all())?,
            Action::Indent => self.state.edit(|editor| editor.indent())?,
            Action::Outdent => self.state.edit(|editor| editor.outdent())?,
            Action::Copy => self.copy()?,
            Action::Cut => self.cut()?,
            Action::Paste => self.paste()?,
            Action::ToggleMouse => self.toggle_mouse(),
            Action::Move(motion) => self.state.edit(|editor| editor.move_cursor(motion))?,
            Action::Select(motion) => self.state.edit(|editor| editor.select(motion))?,
            Action::NewFile => self.state.workspace_mut().new_file(),
            Action::CloseTab => self.close_tab(),
            Action::NextTab => self.state.workspace_mut().next_tab(),
            Action::PreviousTab => self.state.workspace_mut().previous_tab(),
            Action::SplitRight => self.state.workspace_mut().split(Axis::Horizontal),
            Action::SplitDown => self.state.workspace_mut().split(Axis::Vertical),
            Action::FocusNextPane => self.state.workspace_mut().focus_next_pane(),
            Action::FocusPane(direction) => self.state.workspace_mut().focus_direction(direction),
            // Plugin actions need the plugin runtime, which is post-0.1.0.
            Action::Plugin(_) => {}
        }
        Ok(())
    }

    /// A mouse change the terminal has to be told about, once.
    pub(crate) fn take_mouse_change(&mut self) -> Option<bool> {
        self.mouse_change.take()
    }

    /// Moves the keyboard to the explorer, or back to the editor.
    fn toggle_explorer_focus(&mut self) {
        if self.state.focus() == Focus::Explorer {
            self.state.set_focus(Focus::Editor);
        } else if self.state.layout().contains(&ComponentKind::Explorer) {
            self.state.set_focus(Focus::Explorer);
        } else {
            self.state.notify("the layout has no explorer");
        }
    }

    fn explorer_command(&mut self, command: ExplorerCommand) -> IdeResult {
        let chosen = self.state.explorer_mut().apply(command)?;
        self.open_from_explorer(chosen)
    }

    /// Opens a file the user chose in the explorer. The keyboard stays where it
    /// is, so the next file can be picked.
    fn open_from_explorer(&mut self, chosen: Option<std::path::PathBuf>) -> IdeResult {
        let Some(path) = chosen else {
            return Ok(());
        };
        self.state.workspace_mut().open_path(&path)?;
        Ok(())
    }

    /// Mouse input for the explorer. Returns whether it was used up.
    fn handle_explorer_mouse(&mut self, event: MouseEvent) -> bool {
        let (column, row) = (event.column, event.row);
        let over = self.state.explorer().contains(column, row);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) if over => {
                self.state.set_focus(Focus::Explorer);
                let chosen = self.state.explorer_mut().click(column, row);
                let result = match chosen {
                    Ok(chosen) => self.open_from_explorer(chosen),
                    Err(error) => Err(error.into()),
                };
                if let Err(error) = result {
                    self.state.notify_error(error.to_string());
                }
                true
            }
            MouseEventKind::Down(MouseButton::Left) => {
                self.state.set_focus(Focus::Editor);
                false
            }
            // A drag that started in the explorer has nothing to select.
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left)
                if self.state.focus() == Focus::Explorer =>
            {
                true
            }
            MouseEventKind::ScrollUp if over => {
                self.state.explorer_mut().scroll_by(-1);
                true
            }
            MouseEventKind::ScrollDown if over => {
                self.state.explorer_mut().scroll_by(1);
                true
            }
            _ => false,
        }
    }

    fn toggle_mouse(&mut self) {
        self.mouse_enabled = !self.mouse_enabled;
        self.mouse_change = Some(self.mouse_enabled);
        let message = match self.mouse_enabled {
            true => "mouse on (hold shift to select text in the terminal)",
            false => "mouse off",
        };
        self.state.notify(message);
    }

    fn handle_mouse(&mut self, event: MouseEvent) {
        if matches!(event.kind, MouseEventKind::Down(_)) && self.state.dismiss_errors() {
            self.needs_redraw = true;
        }
        if !self.mouse_enabled || self.modal_open() {
            return;
        }
        if self.handle_explorer_mouse(event) {
            self.needs_redraw = true;
            return;
        }
        let extend = event.modifiers.contains(KeyModifiers::SHIFT);
        let sideways = extend;
        let (column, row) = (event.column, event.row);
        let workspace = self.state.workspace_mut();
        let mut closed = None;

        // The wheel scrolls the pane under the pointer; everything else acts
        // on the focused pane (a press also moves focus).
        let result = match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let clicks = self.clicks.register(std::time::Instant::now(), column, row);
                workspace.mouse_press(column, row, extend, clicks)
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                workspace.with_editor(|editor| editor.mouse_drag(column, row))
            }
            // Middle-clicking a tab closes it.
            MouseEventKind::Down(MouseButton::Middle) => {
                closed = workspace.middle_press(column, row);
                Ok(())
            }
            MouseEventKind::Up(MouseButton::Left) => {
                workspace.with_editor(|editor| editor.mouse_release());
                Ok(())
            }
            MouseEventKind::ScrollUp if sideways => {
                workspace.with_editor_at(column, row, |editor| editor.scroll_columns(-1));
                Ok(())
            }
            MouseEventKind::ScrollDown if sideways => {
                workspace.with_editor_at(column, row, |editor| editor.scroll_columns(1));
                Ok(())
            }
            MouseEventKind::ScrollUp => {
                workspace.with_editor_at(column, row, |editor| editor.scroll_lines(-1));
                Ok(())
            }
            MouseEventKind::ScrollDown => {
                workspace.with_editor_at(column, row, |editor| editor.scroll_lines(1));
                Ok(())
            }
            MouseEventKind::ScrollLeft => {
                workspace.with_editor_at(column, row, |editor| editor.scroll_columns(-1));
                Ok(())
            }
            MouseEventKind::ScrollRight => {
                workspace.with_editor_at(column, row, |editor| editor.scroll_columns(1));
                Ok(())
            }
            _ => return,
        };

        if let Some(result) = closed {
            self.warn_if_unsaved(result);
        }
        if let Err(error) = result {
            self.state.notify_error(error.to_string());
        }
        self.needs_redraw = true;
    }

    /// Whether an overlay is open and taking all input.
    fn modal_open(&self) -> bool {
        self.state.quit_prompt().is_some()
            || self.state.palette().is_some()
            || self.state.finder().is_some()
    }

    /// Opens the file finder on the project and starts the walk that feeds it.
    fn open_finder(&mut self) {
        let Some(events) = self.events.clone() else {
            self.state
                .notify_error("the file finder needs the event loop, which is not running");
            return;
        };
        let root = match self.state.explorer().root_path() {
            Some(root) => root.to_path_buf(),
            None => std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        };
        self.scans_started += 1;
        let scan = self.scans_started;
        let handle = finder::start_scan(root.clone(), scan, events);
        self.state
            .open_finder(Finder::new(root, scan, Some(handle)));
    }

    fn handle_finder_key(&mut self, key: KeyEvent) {
        let Some(command) = palette::Command::from_key(key) else {
            return;
        };
        let Some(mut finder) = self.state.take_finder() else {
            return;
        };
        let mut keep_open = true;
        let mut chosen = None;
        match command {
            palette::Command::Insert(c) => finder.insert(c),
            palette::Command::Backspace => finder.backspace(),
            palette::Command::ClearQuery => finder.clear_query(),
            palette::Command::Previous => finder.select_previous(),
            palette::Command::Next => finder.select_next(),
            palette::Command::Cancel => keep_open = false,
            palette::Command::Run => {
                chosen = finder.selected_path();
                keep_open = chosen.is_none();
            }
        }
        if keep_open {
            self.state.open_finder(finder);
        }
        self.needs_redraw = true;
        let Some(path) = chosen else {
            return;
        };
        // Unlike the explorer, the finder is for getting to a file: the editor
        // gets the keyboard.
        self.state.set_focus(Focus::Editor);
        if let Err(error) = self.state.workspace_mut().open_path(&path) {
            self.state.notify_error(error.to_string());
        }
    }

    fn open_palette(&mut self) {
        let entries = Action::palette_actions()
            .into_iter()
            .filter_map(|action| {
                let title = action.title()?;
                let keys = self
                    .keymap
                    .keys_for(&action, self.contexts())
                    .or_else(|| self.keymap.keys_for(&action, &PALETTE_CONTEXTS));
                Some(Entry {
                    title,
                    action,
                    keys,
                })
            })
            .collect();
        self.state.open_palette(Palette::new(entries));
    }

    fn handle_palette_key(&mut self, key: KeyEvent) {
        let Some(command) = palette::Command::from_key(key) else {
            return;
        };
        let Some(mut palette) = self.state.take_palette() else {
            return;
        };
        let mut keep_open = true;
        let mut chosen = None;
        match command {
            palette::Command::Insert(c) => palette.insert(c),
            palette::Command::Backspace => palette.backspace(),
            palette::Command::ClearQuery => palette.clear_query(),
            palette::Command::Previous => palette.select_previous(),
            palette::Command::Next => palette.select_next(),
            palette::Command::Cancel => keep_open = false,
            palette::Command::Run => {
                chosen = palette.selected_entry().map(|entry| entry.action.clone());
                keep_open = chosen.is_none();
            }
        }
        if keep_open {
            self.state.open_palette(palette);
        }
        self.needs_redraw = true;
        if let Some(action) = chosen {
            self.run_action(action);
        }
    }

    /// Quits, after asking what to do with documents that have unsaved
    /// changes.
    fn quit(&mut self) {
        let dirty = self.state.workspace().dirty_documents();
        if dirty.is_empty() {
            self.flow = Flow::Quit;
        } else {
            self.state.open_quit_prompt(QuitPrompt::new(dirty));
        }
    }

    fn handle_prompt_key(&mut self, key: KeyEvent) {
        let Some(command) = quit_prompt::Command::from_key(key) else {
            return;
        };
        let Some(mut prompt) = self.state.take_quit_prompt() else {
            return;
        };

        let mut keep_open = true;
        match command {
            quit_prompt::Command::Previous => prompt.select_previous(),
            quit_prompt::Command::Next => prompt.select_next(),
            quit_prompt::Command::Cancel => keep_open = false,
            quit_prompt::Command::Discard => prompt.remove_selected(),
            quit_prompt::Command::DiscardAll => self.flow = Flow::Quit,
            quit_prompt::Command::Save => {
                if let Some(item) = prompt.selected_item().cloned() {
                    self.save_for_prompt(&mut prompt, &item.name, item.document);
                }
            }
            quit_prompt::Command::SaveAll => {
                while let Some(item) = prompt.selected_item().cloned() {
                    if !self.save_for_prompt(&mut prompt, &item.name, item.document) {
                        break;
                    }
                }
            }
        }
        if prompt.is_empty() {
            self.flow = Flow::Quit;
        } else if keep_open && self.flow != Flow::Quit {
            self.state.open_quit_prompt(prompt);
        }
        self.needs_redraw = true;
    }

    /// Saves one document of the prompt and drops it from the list. On
    /// failure it stays listed and the error is shown.
    fn save_for_prompt(&mut self, prompt: &mut QuitPrompt, name: &str, id: DocumentId) -> bool {
        match self.state.workspace_mut().save_document(id) {
            Ok(SaveOutcome::Saved) => {
                prompt.remove_selected();
                true
            }
            Ok(SaveOutcome::NeedsConfirmation(name)) => {
                self.state.notify_error(overwrite_warning(&name));
                false
            }
            Err(error) => {
                self.state.notify_error(format!("{name}: {error}"));
                false
            }
        }
    }

    fn close_tab(&mut self) {
        let result = self.state.workspace_mut().close_tab();
        self.warn_if_unsaved(result);
    }

    fn warn_if_unsaved(&mut self, result: CloseResult) {
        if let CloseResult::Unsaved(name) = result {
            self.state.notify(format!(
                "{name} has unsaved changes; close again to discard them"
            ));
        }
    }

    fn copy(&mut self) -> IdeResult {
        let register = self.state.edit(|editor| editor.copy())?;
        self.clipboard.set(register);
        Ok(())
    }

    fn cut(&mut self) -> IdeResult {
        let register = self.state.edit(|editor| editor.cut())?;
        self.clipboard.set(register);
        Ok(())
    }

    fn paste(&mut self) -> IdeResult {
        let Some(fetched) = self.clipboard.get() else {
            self.state.notify("nothing to paste");
            return Ok(());
        };
        self.state.edit(|editor| editor.paste(&fetched.register))?;
        if fetched.notice == Some(Notice::SystemUnreadable) {
            self.state
                .notify("system clipboard is not readable here; pasted the editor's own copy");
        }
        Ok(())
    }

    /// Text the terminal pasted (bracketed paste, e.g. ctrl+shift+v or a middle
    /// click). Goes straight into the editor; it neither reads nor changes the
    /// clipboard.
    fn paste_from_terminal(&mut self, text: String) {
        if self.modal_open() {
            return;
        }
        if !self.state.workspace().has_tabs() {
            self.state.notify(NO_FILE_MESSAGE);
            self.needs_redraw = true;
            return;
        }
        let paste = Register::charwise(text);
        if let Err(error) = self.state.edit(|editor| editor.paste(&paste)) {
            self.state.notify_error(error.to_string());
        }
        self.needs_redraw = true;
    }

    fn save(&mut self) -> IdeResult {
        match self.state.workspace_mut().save_active()? {
            SaveOutcome::Saved => {
                let name = self.state.editor().display_name();
                self.state.notify(format!("saved {name}"));
            }
            SaveOutcome::NeedsConfirmation(name) => {
                self.state.notify_error(overwrite_warning(&name))
            }
        }
        Ok(())
    }

    pub(super) fn mark_drawn(&mut self) {
        self.needs_redraw = false;
    }
}

/// What saving says when the file changed on disk since it was read.
fn overwrite_warning(name: &str) -> String {
    format!("{name} changed on disk since you opened it; save again to overwrite it")
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
