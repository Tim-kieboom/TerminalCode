//! What keys do: which bindings apply, typing, and running an action.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::App;
use crate::app::state::{Focus, Popup};
use crate::components::workspace::CloseScope;
use crate::error::IdeResult;
use crate::event::action::Action;
use crate::files::entries::EntryKind;
use crate::keymap::{Context, Expiry, KeyChord, Keymap, Lookup, Outcome, Resolution, Resolver};
use crate::settings::Settings;
use crate::ui::layout::Axis;

/// Shown when an editing action arrives while every tab is closed.
pub(super) const NO_FILE_MESSAGE: &str = "no open file (ctrl+n opens a new one)";
/// Contexts whose bindings apply to the focused editor, most specific first.
pub(super) const EDITOR_CONTEXTS: [Context; 2] = [Context::Editor, Context::Global];
pub(super) const EXPLORER_CONTEXTS: [Context; 2] = [Context::Explorer, Context::Global];
pub(super) const ERRORS_CONTEXTS: [Context; 2] = [Context::Errors, Context::Global];
/// A plugin's pane: the user's bindings for plugins, then the keys the plugin
/// declared, then the global ones.
pub(super) const PLUGIN_CONTEXTS: [Context; 3] =
    [Context::Plugin, Context::PluginDefaults, Context::Global];
/// Where the palette looks for the keys shown beside an action.
pub(super) const PALETTE_CONTEXTS: [Context; 3] =
    [Context::Explorer, Context::Editor, Context::Global];

/// The keys: the bindings, what a half-typed sequence has so far, and how long it
/// waits for the rest.
#[derive(Debug)]
pub(super) struct Keyboard {
    keymap: Keymap,
    resolver: Resolver,
    /// How long it waits for the next chord; `None` waits for ever.
    sequence_timeout: Option<Duration>,
    /// Whether a sequence is half typed.
    pending: bool,
    /// When the half-typed sequence, if there is one and the wait has an end,
    /// gives up waiting.
    pending_deadline: Option<Instant>,
}

impl Keyboard {
    /// Uses `keymap` from now on. A half-typed sequence is dropped, since it
    /// belonged to the old one.
    pub(super) fn set_keymap(&mut self, keymap: Keymap) {
        self.keymap = keymap;
        self.resolver = Resolver::default();
        self.pending = false;
        self.pending_deadline = None;
    }

    pub(super) fn new(keymap: Keymap) -> Self {
        Self {
            keymap,
            resolver: Resolver::default(),
            sequence_timeout: Settings::default().sequence_timeout,
            pending: false,
            pending_deadline: None,
        }
    }

    #[cfg(test)]
    pub(super) fn with_sequence_timeout(mut self, timeout: Duration) -> Self {
        self.sequence_timeout = Some(timeout);
        self
    }

    /// How long a half-typed sequence waits from now on; `None` for ever.
    pub(super) fn set_sequence_timeout(&mut self, timeout: Option<Duration>) {
        self.sequence_timeout = timeout;
    }

    /// When the half-typed key sequence, if any, gives up waiting.
    pub(super) fn pending_deadline(&self) -> Option<Instant> {
        self.pending_deadline
    }

    /// The action that `chord` alone is bound to in `contexts`, if any.
    pub(super) fn action_for(&self, contexts: &[Context], chord: KeyChord) -> Option<&Action> {
        match self.keymap.find(contexts, &[chord]) {
            Lookup::Found { action, .. } => action,
            Lookup::NotFound => None,
        }
    }

    /// Whether a key sequence is half typed, so the next key belongs to it.
    pub(super) fn is_pending(&self) -> bool {
        self.pending
    }

    /// The chords of the half-typed sequence as typed, for the status bar.
    pub(super) fn pending_keys(&self) -> Option<String> {
        let chords = self.resolver.pending();
        (!chords.is_empty()).then(|| {
            let chords: Vec<String> = chords.iter().map(ToString::to_string).collect();
            chords.join(" ")
        })
    }

    /// Takes the next chord. The sequence starts waiting for its deadline when
    /// the chord left it half typed, and stops when it did not.
    pub(super) fn feed(&mut self, contexts: &[Context], chord: KeyChord) -> Resolution {
        let resolution = self.resolver.feed(&self.keymap, contexts, chord);
        self.pending = matches!(resolution.outcome, Outcome::Pending);
        self.pending_deadline = self
            .pending
            .then(|| self.sequence_timeout.map(|wait| Instant::now() + wait))
            .flatten();
        resolution
    }

    /// Gives up on the half-typed sequence.
    fn expire(&mut self, contexts: &[Context]) -> Expiry {
        self.pending = false;
        self.pending_deadline = None;
        self.resolver.expire(&self.keymap, contexts)
    }

    /// Puts in the keys of the plugin whose pane has the keyboard (see
    /// [`Keymap::set_plugin_defaults`]).
    pub(super) fn set_plugin_defaults(
        &mut self,
        bindings: &[(String, Action)],
    ) -> Vec<crate::keymap::KeymapError> {
        self.keymap.set_plugin_defaults(bindings)
    }

    /// What the keymap knows about `keys` in `contexts`.
    pub(super) fn find(
        &self,
        contexts: &[Context],
        keys: &[crate::keymap::KeyChord],
    ) -> crate::keymap::Lookup<'_> {
        self.keymap.find(contexts, keys)
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

        // A key takes what the server said about a word away (a box pinned with f6
        // scrolls with the arrow and page keys); Esc does nothing else then.
        if self.handle_hover_key(key) {
            return;
        }

        // `esc` in the pane of a plugin that is fetching, pulling or pushing stops that.
        if key.code == KeyCode::Esc
            && key.kind == KeyEventKind::Press
            && !self.state.plugin_input()
            && self.esc_stops_remote()
        {
            self.needs_redraw = true;
            return;
        }

        // The popup of a plugin: `esc` closes it, unless the plugin is typing into a
        // line (then `esc` ends that first); every other key goes the way the keys
        // of a plugin's pane go.
        if self.state.plugin_popup().is_some()
            && key.code == KeyCode::Esc
            && !self.state.plugin_input()
            && !self.keyboard.is_pending()
        {
            if key.kind == KeyEventKind::Press {
                self.close_plugin_popup();
            }
            return;
        }

        // An open popup has all the keys.
        match self.state.popup() {
            Popup::None | Popup::PluginView(_) => {}
            Popup::PluginList(_) => return self.handle_plugin_list_key(key),
            Popup::PluginReadme(_) => return self.handle_plugin_readme_key(key),
            Popup::PluginKey(_) => return self.handle_plugin_key_capture(key),
            Popup::QuitPrompt(_) => return self.handle_prompt_key(key),
            Popup::Confirm(_) => return self.handle_confirm_key(key),
            Popup::NewEntry(_) => return self.handle_name_key(key),
            Popup::Menu(_) => return self.handle_menu_key(key),
            Popup::Diagnostics(_) => return self.handle_diagnostic_popup_key(key),
            Popup::Definitions(_) => return self.handle_definitions_key(key),
            Popup::Actions(_) => return self.handle_actions_key(key),
            Popup::RootPassword(_) => return self.handle_root_key(key),
            Popup::Palette(_) => return self.handle_palette_key(key),
            Popup::Finder(_) => return self.handle_finder_key(key),
            Popup::Search(_) => return self.handle_search_key(key),
        }

        // The find bar is not a popup: it has the keys while the user is typing in it, and the
        // editor is used as usual while it only stays open. `esc` closes it from there too.
        if self.state.find().is_some() {
            if self.find_has_keys() {
                return self.handle_find_key(key);
            }
            if key.code == KeyCode::Esc
                && key.modifiers == KeyModifiers::NONE
                && self.state.focus() == Focus::Editor
                && !self.keyboard.is_pending()
            {
                if key.kind == KeyEventKind::Press {
                    self.close_find();
                }
                return;
            }
        }

        // Escape gives up a row being dragged, and does nothing else then.
        if key.code == KeyCode::Esc && self.state.explorer().is_dragging() {
            self.state.explorer_mut().cancel_drag();
            self.needs_redraw = true;
            return;
        }

        // Escape gives up a quit that waits for its saves; the saves go on.
        if key.code == KeyCode::Esc && key.kind == KeyEventKind::Press && self.cancel_waiting_quit()
        {
            self.needs_redraw = true;
            return;
        }

        // A plugin's line to type into has every key but `esc`.
        if self.state.plugin_input() && !self.keyboard.is_pending() {
            return self.handle_plugin_input_key(key);
        }

        // The terminal keeps the keyboard, except for the keys behind its prefix.
        if self.state.focus() == Focus::Terminal && !self.keyboard.is_pending() {
            return self.handle_terminal_key(key);
        }

        if self.handle_completion_key(key) {
            return;
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
            Outcome::Unbound(chord) if self.state.focus() == Focus::Terminal => {
                self.state.notify(format!("ctrl+b {chord} is not bound"))
            }
            Outcome::Unbound(chord) => self.type_chord(chord),
        }
    }

    /// Gives up on a half-typed key sequence: runs it if it is a binding by
    /// itself, otherwise types what can be typed.
    pub(crate) fn expire_pending(&mut self) {
        if self.expire_terminal_prefix() {
            return;
        }
        let contexts = self.contexts();
        match self.keyboard.expire(contexts) {
            Expiry::Nothing => {}
            Expiry::Fire(action) => self.run_action(action),
            // A half-typed sequence that ran out is dropped, not typed.
            Expiry::Discard(_) => {}
        }
        self.show_pending_keys();
    }

    /// The contexts whose bindings apply to the component with the keyboard.
    pub(super) fn contexts(&self) -> &'static [Context] {
        match self.state.focus() {
            Focus::Editor => &EDITOR_CONTEXTS,
            Focus::Explorer => &EXPLORER_CONTEXTS,
            Focus::Terminal => &EDITOR_CONTEXTS,
            Focus::Errors => &ERRORS_CONTEXTS,
            Focus::Plugin => &PLUGIN_CONTEXTS,
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

    pub(crate) fn run_action(&mut self, action: Action) {
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
            Action::NextSearchResult => self.step_search_result(1),
            Action::FindNext => self.find_step(true),
            Action::FindPrevious => self.find_step(false),
            Action::NextDiagnostic => self.step_diagnostic(true),
            Action::PreviousDiagnostic => self.step_diagnostic(false),
            Action::ShowDiagnostics => self.show_diagnostics(),
            Action::ShowHover => self.show_hover(),
            Action::JumpBack => self.jump_back(),
            Action::JumpForward => self.jump_forward(),
            Action::GoToDefinition => self.go_to_definition(),
            Action::RenameSymbol => self.rename_symbol(),
            Action::CodeActions => self.code_action(),
            Action::FormatDocument => self.format_document(),
            Action::FormatSelection => self.format_selection(),
            Action::PreviousSearchResult => self.step_search_result(-1),
            Action::FindInFile => self.open_find(),
            Action::ReplaceInFile => self.open_replace(),
            Action::FocusExplorer => self.toggle_explorer_focus(),
            Action::ToggleExplorer => self.toggle_explorer(),
            Action::ToggleStatusBar => self.state.toggle_status_bar(),
            Action::ToggleTerminal => self.toggle_terminal(),
            Action::TerminalNewTab => self.new_terminal_tab(),
            Action::TerminalNextTab => self.step_terminal_tab(1),
            Action::TerminalPreviousTab => self.step_terminal_tab(-1),
            Action::TerminalCloseTab => self.close_terminal_tab(),
            Action::TerminalCopy => self.copy_terminal_selection(true),
            Action::TogglePluginView(id) => self.toggle_plugin_view(&id),
            Action::FocusPluginView(id) => self.focus_plugin_view(&id),
            Action::ShowPlugins => self.show_plugins(),
            Action::TrustFolder => self.ask_to_trust_folder(),
            Action::OpenPluginPopup(id) => self.open_plugin_popup(&id),
            Action::PluginPlaced(chosen) => self.plugin_placed(&chosen),
            Action::TogglePlugin(id) => self.toggle_plugin(&id),
            // Focus went to the editor before the match (see `focuses_editor`).
            Action::FocusEditor => {}
            Action::Explorer(command) => self.explorer_command(command)?,
            Action::Errors(command) => self.errors_command(command),
            Action::ShowErrors => self.toggle_errors(),
            Action::ShowLspServer => self.show_lsp_server_of_file(),
            Action::OpenMenu => self.open_main_menu(),
            Action::OpenFile => self.ask_for_file(),
            Action::OpenRecentFile(path) => self.open_recent_file(&path),
            Action::OpenRecentFolder(path) => self.open_recent_folder(&path),
            Action::ClearRecent => self.clear_recent(),
            Action::OpenFolder => self.ask_for_folder(),
            Action::ToggleMenuBar => self.toggle_menu_bar(),
            Action::DeleteSelected => self.delete_selected(),
            Action::CreateFile => self.create_entry(EntryKind::File),
            Action::CreateFolder => self.create_entry(EntryKind::Folder),
            Action::Rename => self.rename_selected(),
            Action::MoveEntry => self.move_selected(),
            Action::ContextMenu => self.open_context_menu(),
            Action::GoToLine => self.open_go_to_line(),
            Action::RevealActiveFile => self.reveal_active_file(),
            Action::OpenConfig(file) => self.open_config(file)?,
            Action::ResetConfig(file) => self.ask_to_reset_config(file),
            Action::ResetSavedState => self.ask_to_reset_saved_state(),
            Action::Reload => self.reload(),
            Action::UseInstalledTheme(name) => self.use_installed_theme(&name),
            Action::ChooseTheme => self.choose_theme(),
            Action::NewTheme => self.choose_theme_template(),
            Action::NewThemeFrom(template) => self.ask_for_theme_name(&template),
            Action::Save => self.save()?,
            Action::Undo => self.state.edit(|editor| editor.undo())?,
            Action::Redo => self.state.edit(|editor| editor.redo())?,
            Action::InsertText(text) => {
                self.note_typed();
                self.state.edit(|editor| editor.insert_text(&text))?
            }
            Action::InsertNewline => self.state.edit(|editor| editor.insert_newline())?,
            Action::DeleteBackward => self.state.edit(|editor| editor.delete_backward())?,
            Action::DeleteForward => self.state.edit(|editor| editor.delete_forward())?,
            Action::DeleteWordBackward => {
                self.state.edit(|editor| editor.delete_word_backward())?
            }
            Action::DeleteWordForward => self.state.edit(|editor| editor.delete_word_forward())?,
            Action::SelectAll => self.state.edit(|editor| editor.select_all())?,
            Action::NextBarView => self.cycle_bar_view(1),
            Action::PreviousBarView => self.cycle_bar_view(-1),
            Action::MoveLineUp => self.state.edit(|editor| editor.move_lines(-1))?,
            Action::MoveLineDown => self.state.edit(|editor| editor.move_lines(1))?,
            Action::AddCursorAbove => self.state.edit(|editor| editor.add_cursor_vertically(-1))?,
            Action::AddCursorBelow => self.state.edit(|editor| editor.add_cursor_vertically(1))?,
            Action::CollapseCursors => self.state.edit(|editor| {
                editor.collapse_cursors();
                Ok::<(), crate::buffer::BufferError>(())
            })?,
            Action::Indent => self.state.edit(|editor| editor.indent())?,
            Action::Outdent => self.state.edit(|editor| editor.outdent())?,
            Action::IndentLine => self.state.edit(|editor| editor.indent_line())?,
            Action::OutdentLine => self.state.edit(|editor| editor.outdent_line())?,
            Action::ToggleComment => self.toggle_comment(),
            Action::Copy => self.copy()?,
            Action::Cut => self.cut()?,
            Action::Paste => self.paste()?,
            Action::ToggleMouse => self.toggle_mouse(),
            Action::Move(motion) => self.state.edit(|editor| editor.move_cursor(motion))?,
            Action::Select(motion) => self.state.edit(|editor| editor.select(motion))?,
            Action::NewFile => self.state.workspace_mut().new_file(),
            Action::CloseTab => self.close_tab(),
            Action::CloseOtherTabs => self.close_tabs(CloseScope::Others),
            Action::CloseTabsToLeft => self.close_tabs(CloseScope::Left),
            Action::CloseTabsToRight => self.close_tabs(CloseScope::Right),
            Action::TogglePinTab => self.toggle_pin_tab(),
            Action::UnlockForEditing => self.unlock_for_root(),
            Action::CloseAllTabs => self.close_tabs(CloseScope::All),
            Action::ReopenClosedTab => self.reopen_closed_tab(),
            Action::SaveAll => self.save_all(),
            Action::Run => self.run_active(),
            Action::NextTab => self.state.workspace_mut().next_tab(),
            Action::PreviousTab => self.state.workspace_mut().previous_tab(),
            Action::ToggleSidePane => self.toggle_side_pane(),
            Action::ResetPaneSizes => self.reset_pane_sizes(),
            Action::SidePaneWider => self.resize_side_pane(5),
            Action::SidePaneNarrower => self.resize_side_pane(-5),
            Action::PaneGrow => self.resize_focused_pane(50),
            Action::PaneShrink => self.resize_focused_pane(-50),
            Action::SplitRight => self.state.workspace_mut().split(Axis::Horizontal),
            Action::SplitDown => self.state.workspace_mut().split(Axis::Vertical),
            Action::FocusNextPane => self.state.workspace_mut().focus_next_pane(),
            Action::FocusPane(direction) => self.focus_toward(direction),
            Action::Plugin(action) => self.run_plugin_action(&action),
        }
        Ok(())
    }
}

/// Text a key types when nothing is bound to it.
fn typed_text(chord: &KeyChord) -> Option<Action> {
    let typed = chord.typed_char()?;
    Some(Action::InsertText(typed.to_string().into()))
}

impl App {
    /// Tells the status bar which keys of a half-typed sequence there are.
    pub(super) fn show_pending_keys(&mut self) {
        if self.state.set_pending_keys(self.keyboard.pending_keys()) {
            self.needs_redraw = true;
        }
    }
}

#[cfg(test)]
mod tests;
