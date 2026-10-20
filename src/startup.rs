//! What is read from the user's files before the editor starts.

use std::path::Path;

use crate::app::state::AppState;
use crate::config::{self, ConfigFile, ConfigWarning, ThemeChoice};
use crate::keymap::Keymap;
use crate::persist::recent::{self, Recent};
use crate::persist::saved_state::{self, SavedState};
use crate::settings::Settings;
use crate::settings::update::update_file;
use crate::terminal::KeyboardSupport;
use crate::ui::ComponentKind;

/// The parts of the user's configuration the app keeps.
pub(crate) struct UserConfig {
    pub(crate) settings: Settings,
    pub(crate) theme: ThemeChoice,
    pub(crate) keymap: Keymap,
    /// Which panes the user hid or showed last time.
    pub(crate) saved: SavedState,
    /// The files and folders opened lately.
    pub(crate) recent: Recent,
}

/// Reads the settings, theme, layout and keymap files in `config_dir`, puts the
/// theme and layout in `state` and returns the rest. Whatever cannot be used is
/// shown as an error and the built-in default stands in for it.
pub(crate) fn load_user_config(
    state: &mut AppState,
    config_dir: Option<&Path>,
    keyboard: KeyboardSupport,
) -> UserConfig {
    let path = |file: ConfigFile| config_dir.map(|dir| file.path_in(dir));
    let (mut saved, saved_warning) = saved_state::load(config_dir);
    // Settings a new version introduced are added to the user's file before it is read.
    let update = path(ConfigFile::Settings)
        .map(|settings| update_file(&settings, &mut saved, false))
        .unwrap_or_default();
    if update.remember
        && let Some(dir) = config_dir
        && let Err(error) = saved_state::save(dir, &saved)
    {
        state.notify_error(format!(
            "{}: {error}; the settings added will be offered again",
            saved_state::path_in(dir).display()
        ));
    }
    if let Some(info) = update.info {
        state.notify(info);
    }
    let mut report = |warning: Option<ConfigWarning>| {
        if let Some(warning) = warning {
            state.notify_error(warning.to_string());
        }
    };

    report(update.warning);
    let loaded = config::load_settings(path(ConfigFile::Settings).as_deref());
    report(loaded.warning);
    let settings = loaded.settings.unwrap_or_default();

    let (theme, warning) = config::startup_theme(settings.theme.as_deref(), config_dir);
    report(warning);
    let loaded_theme = config::load_chosen_theme(&theme, config_dir);
    report(loaded_theme.warning);

    let loaded_layout = config::load_layout(path(ConfigFile::Layout).as_deref());
    report(loaded_layout.warning);

    let loaded_keymap = config::load_keymap(path(ConfigFile::Keymap).as_deref(), keyboard);
    report(loaded_keymap.warning);

    report(saved_warning);
    let (recent, warning) = recent::load(config_dir);
    report(warning);
    // What the user did last time wins over what the layout file says.
    for kind in [
        ComponentKind::Explorer,
        ComponentKind::Terminal,
        ComponentKind::StatusBar,
        ComponentKind::MenuBar,
    ] {
        let shown = saved.shown(&kind).or_else(|| {
            loaded_layout
                .layout
                .starts_hidden(&kind)
                .map(|hidden| !hidden)
        });
        if let Some(shown) = shown {
            state.set_shown(&kind, shown);
        }
    }

    // The pane right of the editor: what the user did last time, else what a `RightBar`
    // of the layout file says (`hidden: true`).
    let side_shown = saved.side_shown().or_else(|| {
        let layout = &loaded_layout.layout;
        let anchor = layout.right_bar_anchor()?;
        layout.starts_hidden(&anchor).map(|hidden| !hidden)
    });
    if let Some(shown) = side_shown {
        state.set_side_pane_shown(shown);
    }

    state.set_theme(loaded_theme.theme);
    state.set_layout(loaded_layout.layout);
    UserConfig {
        settings,
        theme,
        keymap: loaded_keymap.keymap,
        saved,
        recent,
    }
}
