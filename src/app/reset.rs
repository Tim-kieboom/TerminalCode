//! Putting a config file back to the built-in default: ask first, keep the old
//! file as `NAME.bak`, then apply the default.

use super::App;
use crate::components::popups::confirm::Confirm;
use crate::config::{self, ConfigFile};
use crate::persist::saved_state;

impl App {
    pub(super) fn ask_to_reset_config(&mut self, file: ConfigFile) {
        if self.config_dir.is_none() {
            return self.no_config_directory();
        }
        self.state.open_confirm(Confirm::reset_config(file));
    }

    pub(super) fn ask_to_reset_saved_state(&mut self) {
        if self.config_dir.is_none() {
            return self.no_config_directory();
        }
        self.state
            .open_confirm(Confirm::reset_saved_state(saved_state::STATE_FILE));
    }

    fn no_config_directory(&mut self) {
        self.state
            .notify_error("there is no config directory on this system");
    }

    /// Replaces the file with the built-in default and applies it, as if it had
    /// been saved.
    pub(super) fn reset_config(&mut self, file: ConfigFile) {
        let Some(dir) = self.config_dir.clone() else {
            return self.no_config_directory();
        };
        let path = file.path_in(&dir);
        match config::reset_with_backup(&path, file.default_content()) {
            Ok(backup) => {
                self.reload();
                self.told_reset(file.file_name(), backup.is_some());
            }
            Err(error) => self.reset_failed(&path, &error),
        }
    }

    /// Forgets which panes were hidden or shown. They stay as they are on screen
    /// and start the way the layout says next time.
    pub(super) fn reset_saved_state(&mut self) {
        let Some(dir) = self.config_dir.clone() else {
            return self.no_config_directory();
        };
        let path = saved_state::path_in(&dir);
        match config::reset_with_backup(&path, &saved_state::empty_text()) {
            Ok(backup) => {
                self.forget_saved_panes();
                self.told_reset(saved_state::STATE_FILE, backup.is_some());
            }
            Err(error) => self.reset_failed(&path, &error),
        }
    }

    fn told_reset(&mut self, name: &str, kept: bool) {
        let note = match kept {
            true => format!("; the old one is {name}.bak"),
            false => String::new(),
        };
        self.state
            .notify(format!("reset {name} to the default{note}"));
        self.needs_redraw = true;
    }

    fn reset_failed(&mut self, path: &std::path::Path, error: &std::io::Error) {
        self.state.notify_error(format!(
            "{}: {error}; the file was not reset",
            path.display()
        ));
    }
}
