//! What keys do: which bindings apply, typing, and running an action.

use std::time::{Duration, Instant};

use crossterm::event::{KeyEvent, KeyEventKind};

use super::App;
use crate::app::state::{Focus, Popup};
use crate::error::IdeResult;
use crate::event::action::Action;
use crate::keymap::{Context, Expiry, KeyChord, Keymap, Outcome, Resolution, Resolver};
use crate::ui::layout::Axis;

/// Shown when an editing action arrives while every tab is closed.
pub(super) const NO_FILE_MESSAGE: &str = "no open file (ctrl+n opens a new one)";
/// Contexts whose bindings apply to the focused editor, most specific first.
const EDITOR_CONTEXTS: [Context; 2] = [Context::Editor, Context::Global];
const EXPLORER_CONTEXTS: [Context; 2] = [Context::Explorer, Context::Global];
/// Where the palette looks for the keys shown beside an action.
pub(super) const PALETTE_CONTEXTS: [Context; 3] =
    [Context::Explorer, Context::Editor, Context::Global];

/// The keys: the bindings, what a half-typed sequence has so far, and how long it
/// waits for the rest.
#[derive(Debug)]
pub(super) struct Keyboard {
    keymap: Keymap,
    resolver: Resolver,
    sequence_timeout: Duration,
    /// When the half-typed sequence, if there is one, gives up waiting.
    pending_deadline: Option<Instant>,
}

impl Keyboard {
    /// How long a half-typed key sequence waits for its next chord.
    const SEQUENCE_TIMEOUT: Duration = Duration::from_millis(1000);

    pub(super) fn new(keymap: Keymap) -> Self {
        Self {
            keymap,
            resolver: Resolver::default(),
            sequence_timeout: Self::SEQUENCE_TIMEOUT,
            pending_deadline: None,
        }
    }

    #[cfg(test)]
    pub(super) fn with_sequence_timeout(mut self, timeout: Duration) -> Self {
        self.sequence_timeout = timeout;
        self
    }

    /// When the half-typed key sequence, if any, gives up waiting.
    pub(super) fn pending_deadline(&self) -> Option<Instant> {
        self.pending_deadline
    }

    /// Takes the next chord. The sequence starts waiting for its deadline when
    /// the chord left it half typed, and stops when it did not.
    fn feed(&mut self, contexts: &[Context], chord: KeyChord) -> Resolution {
        let resolution = self.resolver.feed(&self.keymap, contexts, chord);
        self.pending_deadline = matches!(resolution.outcome, Outcome::Pending)
            .then(|| Instant::now() + self.sequence_timeout);
        resolution
    }

    /// Gives up on the half-typed sequence.
    fn expire(&mut self, contexts: &[Context]) -> Expiry {
        self.pending_deadline = None;
        self.resolver.expire(&self.keymap, contexts)
    }

    /// The shortest keys bound to `action` in `contexts`, for showing the user.
    pub(super) fn keys_for(&self, action: &Action, contexts: &[Context]) -> Option<String> {
        self.keymap.keys_for(action, contexts)
    }
}

impl App {
    pub(super) fn handle_key(&mut self, key: KeyEvent) {
        // An error has had its moment once the user acts. Dismissed before the key
        // is handled, so an error that this very key causes stays, and the key
        // itself still does its job.
        if key.kind == KeyEventKind::Press && self.state.dismiss_errors() {
            self.needs_redraw = true;
        }

        // An open popup has all the keys.
        match self.state.popup() {
            Popup::None => {}
            Popup::QuitPrompt(_) => return self.handle_prompt_key(key),
            Popup::Confirm(_) => return self.handle_confirm_key(key),
            Popup::Palette(_) => return self.handle_palette_key(key),
            Popup::Finder(_) => return self.handle_finder_key(key),
            Popup::Search(_) => return self.handle_search_key(key),
            Popup::Find(_) => return self.handle_find_key(key),
        }

        let Some(chord) = KeyChord::from_event(key) else {
            return;
        };

        let contexts = self.contexts();
        let resolution = self.keyboard.feed(contexts, chord);

        for discarded in resolution.discarded {
            self.type_chord(discarded);
        }
        match resolution.outcome {
            Outcome::Action(action) => self.run_action(action),
            Outcome::Pending => {}
            Outcome::Unbound(chord) => self.type_chord(chord),
        }
    }

    /// Gives up on a half-typed key sequence: runs it if it is a binding by
    /// itself, otherwise types what can be typed.
    pub(crate) fn expire_pending(&mut self) {
        let contexts = self.contexts();
        match self.keyboard.expire(contexts) {
            Expiry::Nothing => {}
            Expiry::Fire(action) => self.run_action(action),
            Expiry::Discard(chords) => chords.into_iter().for_each(|chord| self.type_chord(chord)),
        }
    }

    /// The contexts whose bindings apply to the component with the keyboard.
    pub(super) fn contexts(&self) -> &'static [Context] {
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

    pub(super) fn run_action(&mut self, action: Action) {
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
            Action::FindInProject => self.open_search(),
            Action::FindInFile => self.open_find(),
            Action::FocusExplorer => self.toggle_explorer_focus(),
            Action::ToggleExplorer => self.toggle_explorer(),
            Action::ToggleStatusBar => self.state.toggle_status_bar(),
            Action::TogglePluginView(id) => self.toggle_plugin_view(&id),
            Action::Explorer(command) => self.explorer_command(command)?,
            Action::DeleteSelected => self.delete_selected(),
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
}

/// Text a key types when nothing is bound to it.
fn typed_text(chord: &KeyChord) -> Option<Action> {
    let typed = chord.typed_char()?;
    Some(Action::InsertText(typed.to_string().into()))
}

#[cfg(test)]
mod tests;
