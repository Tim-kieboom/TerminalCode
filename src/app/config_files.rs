//! Opening the user's theme, keymap and layout files for editing.

use crate::app::state::Focus;
use crate::components::ComponentKind;
use crate::config::{self, ConfigFile};
use crate::error::{IdeError, IdeResult};
use crate::ui::theme::ThemeVariant;

use super::App;

impl App {
    /// Opens `file` in a tab. A file that does not exist yet is made first from
    /// the built-in default, so there is something to edit; one that does is
    /// left as it is. The theme file opened is the one in use: the installed
    /// theme that was chosen, else `theme.toml`, which is started from the
    /// built-in theme that was chosen. A change applies when the file is saved.
    pub(super) fn open_config(&mut self, file: ConfigFile) -> IdeResult {
        let Some(dir) = &self.config_dir else {
            self.state
                .notify_error("there is no config directory on this system");
            return Ok(());
        };

        if file == ConfigFile::Theme
            && let Some(installed) = self.installed_theme_path()
            && installed.is_file()
        {
            self.state.workspace_mut().open_path(&installed)?;
            return Ok(());
        }

        let path = file.path_in(dir);
        let content = match file {
            ConfigFile::Theme => self.theme_variant.source(),
            _ => file.default_content(),
        };
        let created = file
            .create_with(&path, content)
            .map_err(|error| IdeError::Unknown(format!("{}: {error}", path.display()).into()))?;

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
    /// Reads the user's theme, layout and keymap files again and applies them,
    /// as if the editor had started over (and later the plugins too). A file
    /// that is missing is normal, and one that is invalid is reported and
    /// replaced by the built-in one, exactly as at startup. Open files, tabs
    /// and the terminal are left alone.
    pub(super) fn reload(&mut self) {
        self.config_applied = self.config_hash();
        let path = |file: ConfigFile| self.config_dir.as_deref().map(|dir| file.path_in(dir));

        let installed = self.installed_theme_path();
        let theme = config::load_chosen_theme(
            self.theme_variant,
            installed.as_deref(),
            path(ConfigFile::Theme).as_deref(),
        );
        let layout = config::load_layout(path(ConfigFile::Layout).as_deref());
        let keymap =
            config::load_keymap(path(ConfigFile::Keymap).as_deref(), self.keyboard_support);

        let warnings = [theme.warning, layout.warning, keymap.warning];
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
            self.state.notify_error(warning);
        }
        if failed == 0 {
            self.state.notify("reloaded the theme, layout and keymap");
        }
    }

    /// Switches to a built-in theme, with the user's theme file laid over it,
    /// and keeps it across reloads. The choice is not saved for the next start.
    pub(super) fn use_theme(&mut self, variant: ThemeVariant) {
        self.theme_variant = variant;
        self.installed_theme = None;
        self.apply_chosen_theme(variant.name());
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
        self.installed_theme = Some(name.to_owned());
        self.apply_chosen_theme(name);
    }

    /// The file of the installed theme in use, if one is.
    pub(super) fn installed_theme_path(&self) -> Option<std::path::PathBuf> {
        let name = self.installed_theme.as_deref()?;
        config::installed_theme_path(self.config_dir.as_deref()?, name)
    }

    /// Loads the chosen theme and shows it; `name` is what the message calls it.
    fn apply_chosen_theme(&mut self, name: &str) {
        let path = self
            .config_dir
            .as_deref()
            .map(|dir| ConfigFile::Theme.path_in(dir));
        let installed = self.installed_theme_path();
        let loaded =
            config::load_chosen_theme(self.theme_variant, installed.as_deref(), path.as_deref());

        let mut theme = loaded.theme;
        theme.set_terminal_background(self.state.theme().terminal_background());
        self.state.set_theme(theme);
        self.start_syntax_worker();

        match loaded.warning {
            Some(warning) => self.state.notify_error(warning),
            None => self.state.notify(format!("theme: {name}")),
        }
    }

    /// A hash of what the three config files say now (a missing file counts as
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
            Focus::Editor => false,
        };
        if missing {
            self.state.set_focus(Focus::Editor);
        }
    }
}
