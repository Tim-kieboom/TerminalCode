//! Opening the user's theme, keymap and layout files for editing.

use crate::app::state::Focus;
use crate::buffer::Buffer;
use crate::config::{self, ConfigFile, ConfigWarning, ThemeChoice};
use crate::error::IdeResult;
use crate::pty::Shell;
use crate::settings::Settings;
use crate::settings::update::update_file;
use crate::ui::ComponentKind;

use super::App;
use crate::components::popups::name_prompt::NamePrompt;
use crate::files::entries::EntryError;

impl App {
    /// Opens `file` in a tab. A file that does not exist yet is made first from
    /// the built-in default, so there is something to edit; one that does is
    /// left as it is.
    ///
    /// The theme opened is the one in use, as `themes/NAME.toml`: an installed
    /// theme is opened as it is, and for a built-in one a copy of it is made
    /// there first, named after it, and chosen, so what is edited is what shows.
    /// A change applies when the file is saved.
    pub(super) fn open_config(&mut self, file: ConfigFile) -> IdeResult {
        let Some(dir) = &self.config_dir else {
            self.state
                .notify_error("there is no config directory on this system");
            return Ok(());
        };

        if file == ConfigFile::Theme {
            return self.open_theme_in_use();
        }

        let path = file.path_in(dir);
        let created = match file.create_from_default(&path) {
            Ok(created) => created,
            Err(error) => {
                self.show_defaults_copy(&path, file.default_content(), &error);
                return Ok(());
            }
        };

        self.state.workspace_mut().open_path(&path)?;
        if created {
            self.state.notify(format!(
                "made {} from the defaults; changes apply when it is saved",
                path.display()
            ));
        }
        Ok(())
    }
}

impl App {
    /// The file at `path` could not be made, so its defaults are shown in a
    /// locked tab with no file behind it, to read and copy from. The error that
    /// stopped the file from being made stays on screen.
    fn show_defaults_copy(
        &mut self,
        path: &std::path::Path,
        content: &str,
        error: &std::io::Error,
    ) {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let buffer = Buffer::defaults_copy(content, format!("{name} (defaults)"));
        self.state.workspace_mut().open_buffer(buffer);
        self.state.set_focus(Focus::Editor);
        self.state.notify_error(format!(
            "could not make {}: {error}; showing the defaults, which cannot be edited or saved",
            path.display()
        ));
    }
}

/// Said after a theme is chosen from the palette, which lasts for the run only.
const KEEP_HINT: &str = " (this run only; set \"theme\" in settings.json to keep it)";

impl App {
    /// Opens the file of the theme in use. With none chosen (the fallback is in
    /// use) there is no file, and it says how to make a theme.
    fn open_theme_in_use(&mut self) -> IdeResult {
        let Some(dir) = self.config_dir.clone() else {
            return Ok(());
        };
        let Some(name) = self.theme_choice.installed.clone() else {
            self.state.notify(
                "no theme is chosen, the built-in fallback is in use; \"Theme: New...\" makes one from a template",
            );
            return Ok(());
        };
        let Some(path) = config::installed_theme_path(&dir, &name) else {
            self.state
                .notify_error(format!("{name} is not a usable theme name"));
            return Ok(());
        };
        if !path.exists() {
            self.state.notify_error(format!(
                "{}: the theme file is gone; \"Theme: New...\" makes a new one",
                path.display()
            ));
            return Ok(());
        }
        self.state.workspace_mut().open_path(&path)?;
        Ok(())
    }
}

impl App {
    /// Reads the user's settings, theme, layout and keymap files again and
    /// applies them, as if the editor had started over (and later the plugins
    /// too). A file that is missing is normal, and one that is invalid is
    /// reported and replaced by the built-in one, exactly as at startup. Open
    /// files, tabs and the terminal are left alone. A setting is applied only
    /// when the settings file changed it, so a theme chosen from the palette
    /// stays until then; an invalid settings file leaves the settings as they are.
    pub(super) fn reload(&mut self) {
        // Settings a new version introduced are added first, so what is read next has them.
        let update_warning = self.add_new_settings_to_file();
        self.config_applied = self.config_hash();
        let dir = self.config_dir.clone();
        let path = |file: ConfigFile| dir.as_deref().map(|dir| file.path_in(dir));

        let settings = config::load_settings(path(ConfigFile::Settings).as_deref());
        let settings_warning = settings.warning;
        let theme_warning = settings
            .settings
            .and_then(|settings| self.apply_settings(settings));

        let theme = config::load_chosen_theme(&self.theme_choice, dir.as_deref());
        let layout = config::load_layout(path(ConfigFile::Layout).as_deref());
        let keymap =
            config::load_keymap(path(ConfigFile::Keymap).as_deref(), self.keyboard_support);

        let warnings = [
            update_warning,
            settings_warning,
            theme_warning,
            theme.warning,
            layout.warning,
            keymap.warning,
        ];
        let mut theme = theme.theme;
        // Asking the terminal for its background needs the input stream, which
        // the loop owns now, so the answer from startup is kept.
        theme.set_terminal_background(self.state.theme().terminal_background());

        self.state.set_theme(theme);
        self.state.set_layout(layout.layout);
        self.keyboard.set_keymap(keymap.keymap);
        self.keep_focus_inside_the_layout();
        self.start_syntax_worker();

        let failed = warnings.iter().flatten().count();
        for warning in warnings.into_iter().flatten() {
            self.state.notify_error(warning.to_string());
        }
        if failed == 0 {
            self.state
                .notify("reloaded the settings, theme, layout and keymap");
        }
    }

    /// Adds the settings a new version introduced to the user's `settings.json`
    /// (see `settings::update`), remembering the version. Why it could not, if it
    /// could not, is returned.
    fn add_new_settings_to_file(&mut self) -> Option<ConfigWarning> {
        let path = ConfigFile::Settings.path_in(self.config_dir.as_deref()?);
        let workspace = self.state.workspace();
        let unsaved_tab = workspace
            .document_of_path(&path)
            .and_then(|document| workspace.document_info(document))
            .is_some_and(|(_, _, buffer)| buffer.is_dirty());
        let mut saved = self.saved_state();
        let report = update_file(&path, &mut saved, unsaved_tab);
        if report.remember
            && let Some(version) = saved.settings_version()
        {
            self.remember_settings_version(version);
        }
        if let Some(info) = report.info {
            self.state.notify(info);
        }
        report.warning
    }

    /// Makes `settings` the ones in use. The theme they name is chosen when it
    /// differs from the one the settings named before. A name that is no theme
    /// is returned as a warning and leaves the theme as it is; it also counts as
    /// not applied, so it is tried again at the next reload (the theme file may
    /// have been made in the meantime).
    fn apply_settings(&mut self, mut settings: Settings) -> Option<ConfigWarning> {
        let mut warning = None;
        if settings.theme != self.settings.theme {
            match settings.theme.clone() {
                None => self.theme_choice = ThemeChoice::default(),
                Some(name) => match config::resolve_theme(&name, self.config_dir.as_deref()) {
                    Some(choice) => self.theme_choice = choice,
                    None => {
                        warning = Some(ConfigWarning::NoSuchTheme(name));
                        settings.theme = self.settings.theme.clone();
                    }
                },
            }
        }
        let before = std::mem::replace(&mut self.settings, settings);
        self.apply_runtime_settings(&before);
        warning
    }

    /// Puts the settings other than the theme to work: each one that differs
    /// from `before` (what was applied until now). The shell is for terminal tabs
    /// started from now on, and the mouse is told to the terminal by the loop.
    pub(super) fn apply_runtime_settings(&mut self, before: &Settings) {
        if self.settings.shell != before.shell {
            self.terminal_shell = Shell::chosen(self.settings.shell.as_deref());
        }
        if self.settings.sequence_timeout != before.sequence_timeout {
            self.keyboard
                .set_sequence_timeout(self.settings.sequence_timeout);
        }
        if self.settings.notification_lifetime != before.notification_lifetime {
            self.state
                .set_notification_lifetime(self.settings.notification_lifetime);
        }
        if self.settings.hide_ignored != before.hide_ignored {
            self.state
                .explorer_mut()
                .set_hide_ignored(self.settings.hide_ignored);
            self.needs_redraw = true;
        }
        if self.settings.mouse != before.mouse {
            self.mouse.set(self.settings.mouse);
        }
        if self.settings.language_servers != before.language_servers {
            self.lsp_settings_changed();
        }
        if self.settings.indent_prefs() != before.indent_prefs() {
            let prefs = self.settings.indent_prefs();
            self.state.workspace_mut().set_indent_prefs(prefs);
            self.needs_redraw = true;
        }
    }

    /// Switches to the installed theme `name` (a file in the themes folder),
    /// with the user's theme file over it. Like [`App::use_theme`], for this run.
    pub(super) fn use_installed_theme(&mut self, name: &str) {
        let known = self.config_dir.as_deref().is_some_and(|dir| {
            config::installed_themes(dir)
                .iter()
                .any(|known| known == name)
        });
        if !known {
            self.state
                .notify_error(format!("no installed theme called {name}"));
            return;
        }
        self.theme_choice = ThemeChoice::installed(name);
        self.apply_chosen_theme(KEEP_HINT);
    }

    /// The file of the installed theme in use, if one is.
    pub(super) fn installed_theme_path(&self) -> Option<std::path::PathBuf> {
        self.theme_choice.installed_path(self.config_dir.as_deref())
    }

    /// Loads the chosen theme and shows it, saying `theme: NAME` and then `hint`.
    fn apply_chosen_theme(&mut self, hint: &str) {
        let loaded = config::load_chosen_theme(&self.theme_choice, self.config_dir.as_deref());

        let mut theme = loaded.theme;
        theme.set_terminal_background(self.state.theme().terminal_background());
        self.state.set_theme(theme);
        self.start_syntax_worker();

        match loaded.warning {
            Some(warning) => self.state.notify_error(warning.to_string()),
            None => self
                .state
                .notify(format!("theme: {}{hint}", self.theme_choice.name())),
        }
    }

    /// A hash of what the config files say now (a missing file counts as
    /// empty), to tell a file that was saved from one that was only read: reading
    /// can make the watcher report the file too, and reloading reads it again.
    pub(super) fn config_hash(&self) -> Option<u64> {
        use std::hash::{Hash, Hasher};
        let dir = self.config_dir.as_deref()?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for file in ConfigFile::ALL {
            std::fs::read(file.path_in(dir)).ok().hash(&mut hasher);
        }
        if let Some(installed) = self.installed_theme_path() {
            std::fs::read(installed).ok().hash(&mut hasher);
        }
        Some(hasher.finish())
    }

    /// A new layout may not have the component that has the keyboard; it goes
    /// back to the editor instead of to something that is not there.
    fn keep_focus_inside_the_layout(&mut self) {
        let missing = match self.state.focus() {
            Focus::Explorer => !self.state.layout().contains(&ComponentKind::Explorer),
            Focus::Terminal => !self.state.layout().contains(&ComponentKind::Terminal),
            Focus::Errors => !self.state.layout().contains(&ComponentKind::Errors),
            Focus::Editor => false,
        };
        if missing {
            self.state.set_focus(Focus::Editor);
        }
    }
}

impl App {
    /// Opens the palette on "Theme: New from ...", one entry for each theme that
    /// can be the start of a new one.
    pub(super) fn choose_theme_template(&mut self) {
        self.open_palette();
        if let Some(mut palette) = self.state.take_palette() {
            "Theme: New from ".chars().for_each(|c| palette.insert(c));
            self.state.open_palette(palette);
        }
    }

    /// Asks for the name of a new theme that starts as a copy of `template`.
    pub(super) fn ask_for_theme_name(&mut self, template: &str) {
        if self.config_dir.is_none() {
            self.state
                .notify_error("there is no config directory on this system");
            return;
        }
        self.state
            .open_name_prompt(NamePrompt::new_theme(template.to_owned()));
    }

    /// Makes `themes/NAME.toml` from `template`, chooses it and opens it. A name
    /// that is taken, or cannot be a file name, makes nothing.
    pub(super) fn make_theme(&mut self, template: &str, name: &str) -> Result<(), EntryError> {
        let Some(dir) = self.config_dir.clone() else {
            return Ok(());
        };
        let stem = config::theme_file_stem(name).ok_or_else(|| match name.trim().is_empty() {
            true => EntryError::Empty,
            false => EntryError::BadThemeName(name.trim().to_owned()),
        })?;
        let Some(path) = config::installed_theme_path(&dir, &stem) else {
            return Err(EntryError::BadThemeName(stem));
        };
        let source = config::theme_template(&dir, template)
            .ok_or_else(|| EntryError::BadThemeName(template.to_owned()))?;
        let created = ConfigFile::Theme
            .create_with(&path, &source)
            .map_err(|source| EntryError::Io {
                name: stem.clone(),
                source,
            })?;
        if !created {
            return Err(EntryError::Exists(stem));
        }

        self.theme_choice = ThemeChoice::installed(stem.clone());
        self.apply_chosen_theme("");
        if let Err(error) = self.state.workspace_mut().open_path(&path) {
            self.state.notify_error(error.to_string());
        }
        self.state.set_focus(crate::app::state::Focus::Editor);
        self.state.notify(format!(
            "made themes/{stem}.toml from {template} and chose it; changes apply when it is saved"
        ));
        Ok(())
    }
}
