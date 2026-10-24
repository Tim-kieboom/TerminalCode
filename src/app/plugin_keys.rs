//! A key that opens a plugin, chosen in the manager (`k`): the next key pressed is written
//! to the user's `keymap.toml` as a binding of `open_plugin`, and the keymap is read again.
//! A key that is bound already (in any context, or the start of a sequence) is refused,
//! and so is a letter on its own, which would take it from the text.

use std::fs;
use std::io::Write;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::App;
use crate::components::popups::key_capture::KeyCapture;
use crate::config::ConfigFile;
use crate::event::action::Action;
use crate::keymap::{Context, KeyChord, Lookup};

/// The contexts looked in for a key that is taken.
const ALL_CONTEXTS: [Context; 6] = [
    Context::Global,
    Context::Editor,
    Context::Explorer,
    Context::Errors,
    Context::Debug,
    Context::Plugin,
];

impl App {
    /// The keys that open the plugin `id`, as the user's keymap says.
    pub(super) fn open_key_of(&self, id: &str) -> Option<String> {
        self.keyboard
            .keys_for(&Action::OpenPluginPopup(id.into()), &ALL_CONTEXTS)
    }

    /// Asks for the key that opens the plugin `id`.
    pub(super) fn ask_for_key(&mut self, id: &str) {
        let Some(installed) = self.plugins.manager.installed(id) else {
            self.state
                .notify(format!("plugin {id} is not installed: press i"));
            return;
        };
        if self.config_dir.is_none() {
            self.state.notify("there is no folder for the keymap here");
            return;
        }
        if let Some(keys) = self.open_key_of(id) {
            self.state.notify(format!(
                "{} is opened by {keys}: \"Config: Open Keymap\" changes it",
                installed.manifest.display_name()
            ));
            return;
        }
        let capture = KeyCapture::new(id, installed.manifest.display_name());
        self.state.open_plugin_key(capture);
        self.needs_redraw = true;
    }

    /// The key pressed while the question is open.
    pub(super) fn handle_plugin_key_capture(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        self.needs_redraw = true;
        let Some((id, name)) = self
            .state
            .plugin_key_mut()
            .map(|capture| (capture.id().to_owned(), capture.name().to_owned()))
        else {
            return;
        };
        if key.code == KeyCode::Esc {
            self.show_plugin_list(Some(&id));
            return;
        }
        // The modifier keys on their own are not the key.
        if matches!(key.code, KeyCode::Modifier(_)) {
            return;
        }
        let Some(chord) = KeyChord::from_event(key) else {
            return;
        };
        match self.check_key(chord) {
            Err(why) => {
                if let Some(capture) = self.state.plugin_key_mut() {
                    capture.refuse(why);
                }
            }
            Ok(()) => {
                let result = self.write_open_key(&id, chord);
                self.show_plugin_list(Some(&id));
                match result {
                    Ok(()) => {
                        self.reload();
                        self.state
                            .notify(format!("{chord} opens {name} (written to keymap.toml)"));
                    }
                    Err(error) => self
                        .state
                        .notify_error(format!("the key was not saved: {error}")),
                }
            }
        }
    }

    /// Whether `chord` can open a plugin: it has a modifier or is a function key, and nothing
    /// is bound to it or to a sequence that starts with it.
    fn check_key(&self, chord: KeyChord) -> Result<(), String> {
        let modifiers = chord.modifiers();
        let usable = matches!(chord.code(), KeyCode::F(_))
            || modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER);
        if !usable {
            return Err(format!(
                "{chord} on its own is for typing: use ctrl, alt or super with it, or a function key"
            ));
        }
        for context in ALL_CONTEXTS {
            if let Lookup::Found { action, has_more } = self.keyboard.find(&[context], &[chord]) {
                let used = match action.and_then(Action::title) {
                    Some(title) => title.to_owned(),
                    None if has_more => "a sequence of keys".to_owned(),
                    None => "another command".to_owned(),
                };
                return Err(format!("{chord} is used by {used}: press another key"));
            }
        }
        Ok(())
    }

    /// Adds the binding to the end of the user's `keymap.toml` (the file is made when there
    /// is none), which leaves everything the user wrote as it is.
    fn write_open_key(&self, id: &str, chord: KeyChord) -> std::io::Result<()> {
        let Some(dir) = self.config_dir.as_deref() else {
            return Err(std::io::Error::other("there is no config folder"));
        };
        let path = ConfigFile::Keymap.path_in(dir);
        fs::create_dir_all(dir)?;
        let existing = fs::read_to_string(&path).unwrap_or_default();
        let mut text = String::new();
        if existing.is_empty() {
            text.push_str("# Your keymap: bindings added to the default ones.\n");
        } else if !existing.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&format!(
            "\n[[binding]]\nkeys = \"{chord}\"\naction = {{ open_plugin = \"{id}\" }}\n"
        ));
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?
            .write_all(text.as_bytes())
    }
}
