//! Opening the user's theme, keymap and layout files for editing.

use crate::app::state::Focus;
use crate::components::ComponentKind;
use crate::config::{self, ConfigFile};
use crate::error::{IdeError, IdeResult};

use super::App;

impl App {
    /// Opens `file` in a tab. A file that does not exist yet is made first from
    /// the built-in default, so there is something to edit; one that does is
    /// left as it is. The files are read at startup, so a change applies the
    /// next time the editor starts.
    pub(super) fn open_config(&mut self, file: ConfigFile) -> IdeResult {
        let Some(dir) = &self.config_dir else {
            self.state
                .notify_error("there is no config directory on this system");
            return Ok(());
        };
        let path = file.path_in(dir);

        let created = file
            .create_from_default(&path)
            .map_err(|error| IdeError::Unknown(format!("{}: {error}", path.display()).into()))?;
        self.state.workspace_mut().open_path(&path)?;
        if created {
            self.state.notify(format!(
                "made {} from the defaults; changes apply on the next start or reload",
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
        let path = |file: ConfigFile| self.config_dir.as_deref().map(|dir| file.path_in(dir));

        let theme = config::load_theme(path(ConfigFile::Theme).as_deref());
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
