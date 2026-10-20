//! A yes/no question about something that cannot be undone. Only `y` says yes;
//! Enter, Esc and anything else cancel, so a stray key never destroys anything.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

use crate::config::ConfigFile;
use crate::ui::theme::Theme;

const HELP: &str = "y: yes | esc, enter: cancel";
const MIN_WIDTH: u16 = 46;
const MAX_WIDTH: u16 = 80;

/// What saying yes does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Purpose {
    /// Delete `path` although open tabs of it have unsaved changes.
    DiscardAndDelete(PathBuf),
    /// Delete `path` for good, because it could not be moved to the trash.
    DeletePermanently(PathBuf),
    /// Put the built-in default in place of this config file, keeping the old one as `.bak`.
    ResetConfig(ConfigFile),
    /// Forget which panes were hidden or shown, keeping `state.json` as `.bak`.
    ResetSavedState,
    /// Let the plugin with this id run, with what it asked to be allowed to do.
    EnablePlugin(Box<str>),
    /// Download the plugin with this id (first) from the extra source with this URL (second).
    InstallPlugin(Box<str>, Box<str>),
    /// Replace the plugin with this id by the newer version of the index.
    UpdatePlugin(Box<str>),
    /// Delete the folder of the plugin with this id, which the manager installed.
    RemovePlugin(Box<str>),
}

/// What a key press answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Answer {
    Yes,
    Cancel,
}

impl Answer {
    /// The answer a key gives; `None` for key releases and modified keys.
    pub(crate) fn from_key(key: KeyEvent) -> Option<Self> {
        if key.kind == KeyEventKind::Release {
            return None;
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return None;
        }
        Some(match key.code {
            KeyCode::Char('y' | 'Y') => Self::Yes,
            _ => Self::Cancel,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Confirm {
    title: String,
    lines: Vec<String>,
    purpose: Purpose,
}

impl Confirm {
    /// Asks whether to delete `path` and lose the unsaved changes in `names`.
    pub(crate) fn discard_and_delete(path: &Path, names: &[String]) -> Self {
        let mut lines = vec![format!("Delete {}?", display(path)), String::new()];
        lines.push("These files have unsaved changes that will be lost:".to_owned());
        lines.extend(names.iter().map(|name| format!("  {name}")));
        Self {
            title: " Unsaved changes ".to_owned(),
            lines,
            purpose: Purpose::DiscardAndDelete(path.to_path_buf()),
        }
    }

    /// Asks whether to delete `path` for good; moving it to the trash failed
    /// because of `reason`.
    pub(crate) fn delete_permanently(path: &Path, reason: &str) -> Self {
        Self {
            title: " Cannot move to trash ".to_owned(),
            lines: vec![
                format!(
                    "{} could not be moved to the trash: {reason}",
                    display(path)
                ),
                String::new(),
                "Delete it permanently? This cannot be undone.".to_owned(),
            ],
            purpose: Purpose::DeletePermanently(path.to_path_buf()),
        }
    }

    /// Asks whether to put the built-in default in place of the config file `name`.
    pub(crate) fn reset_config(file: ConfigFile) -> Self {
        Self::reset(file.file_name(), Purpose::ResetConfig(file))
    }

    /// Asks whether to let a plugin run. `allowed` says what it will be able to do,
    /// one line for each; no lines is a plugin that can only draw in its pane.
    pub(crate) fn enable_plugin(id: &str, name: &str, version: &str, allowed: &[String]) -> Self {
        let mut lines = vec![
            format!("Turn on the plugin {name} {version}?"),
            String::new(),
        ];
        match allowed.is_empty() {
            true => lines.push("It can draw in its own pane and nothing else.".to_owned()),
            false => {
                lines.push("It will be able to:".to_owned());
                lines.extend(allowed.iter().map(|line| format!("  - {line}")));
            }
        }
        lines.push(String::new());
        lines.push("It runs apart from the editor and cannot do anything else.".to_owned());
        Self {
            title: " Plugin ".to_owned(),
            lines,
            purpose: Purpose::EnablePlugin(id.into()),
        }
    }

    /// Asks whether to install the plugin `id` from an index that is not the official
    /// one: where it comes from, the address, and the hash it will be held to.
    pub(crate) fn install_from_source(
        id: &str,
        name: &str,
        version: &str,
        label: &str,
        url: &str,
        sha256: &str,
    ) -> Self {
        Self {
            title: " Install plugin from a source ".to_owned(),
            lines: vec![
                format!("Install the plugin {name} {version} from {label}?"),
                String::new(),
                "This is not an official plugin: you added the source yourself, and its owner decides what the plugin does.".to_owned(),
                format!("Index: {url}"),
                format!("Module sha256: {}", sha256.get(..16).unwrap_or(sha256)),
                String::new(),
                "That version is pinned the first time it is installed: if it ever changes, the editor refuses it. You are asked what it may do before it runs.".to_owned(),
            ],
            purpose: Purpose::InstallPlugin(id.into(), url.into()),
        }
    }

    /// Asks whether to replace the plugin `id` by a newer version; `running`: it is
    /// stopped and started again.
    pub(crate) fn update_plugin(
        id: &str,
        name: &str,
        from: &str,
        to: &str,
        source: Option<&str>,
        running: bool,
    ) -> Self {
        let mut lines = vec![format!("Update the plugin {name} from {from} to {to}?")];
        if let Some(source) = source {
            lines.push(format!("It comes from {source}, which is not official."));
        }
        if running {
            lines.push(String::new());
            lines.push("It is running: it stops, and the new version starts.".to_owned());
        }
        Self {
            title: " Update plugin ".to_owned(),
            lines,
            purpose: Purpose::UpdatePlugin(id.into()),
        }
    }

    /// Asks whether to remove the plugin `id` that the manager installed. What was
    /// decided about it (what it may do, where it is) is kept.
    pub(crate) fn remove_plugin(id: &str, name: &str) -> Self {
        Self {
            title: " Remove plugin ".to_owned(),
            lines: vec![
                format!("Remove the plugin {name}?"),
                String::new(),
                "Its folder is deleted. What you decided about it is kept, so installing it again brings it back as it was.".to_owned(),
            ],
            purpose: Purpose::RemovePlugin(id.into()),
        }
    }

    /// Asks whether to forget which panes were hidden or shown.
    pub(crate) fn reset_saved_state(name: &str) -> Self {
        Self::reset(name, Purpose::ResetSavedState)
    }

    fn reset(name: &str, purpose: Purpose) -> Self {
        Self {
            title: " Reset to default ".to_owned(),
            lines: vec![
                format!("Reset {name} to the built-in default?"),
                String::new(),
                format!(
                    "The file you have now is kept as {name}.bak; an older {name}.bak is replaced."
                ),
            ],
            purpose,
        }
    }

    pub(crate) fn purpose(&self) -> &Purpose {
        &self.purpose
    }

    #[cfg(test)]
    pub(crate) fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub(crate) fn render(&self, frame: &mut Frame, theme: &Theme) {
        let screen = frame.area();
        let width = MAX_WIDTH.min(screen.width).max(MIN_WIDTH.min(screen.width));
        let inner_width = usize::from(width.saturating_sub(4)).max(1);
        let wrapped: usize = self
            .lines
            .iter()
            .map(|line| line.chars().count().div_ceil(inner_width).max(1))
            .sum();
        let height = (wrapped as u16 + 4).min(screen.height);
        let area = Rect {
            x: screen.x + (screen.width - width) / 2,
            y: screen.y + (screen.height - height) / 2,
            width,
            height,
        };
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.style("pane.border.focused"))
            .title(Line::styled(&self.title, theme.style("pane.title")));
        let inner = block.inner(area);
        crate::ui::backdrop::clear(frame, theme, area);
        frame.render_widget(block, area);

        let body = Rect {
            height: inner.height.saturating_sub(1),
            ..inner
        };
        let help = Rect {
            y: inner.y + body.height,
            height: inner.height - body.height,
            ..inner
        };
        let lines: Vec<Line> = self.lines.iter().map(|l| Line::from(l.as_str())).collect();
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), body);
        frame.render_widget(Paragraph::new(HELP), help);
    }
}

/// The name of `path` for the question; the whole path when it has no name.
fn display(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}
