//! The explorer and the other panels that can be shown, hidden or given the
//! keyboard.

use std::path::Path;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use super::App;
use crate::app::state::Focus;
use crate::components::explorer::{Explorer, ExplorerCommand, Release};
use crate::components::workspace::{FocusDirection, neighbor_score};
use crate::error::IdeResult;
use crate::files::paths::canonical;
use crate::ids::PaneId;
use crate::ui::{ComponentKind, PluginViewId};
use ratatui::layout::Rect;

impl App {
    /// Moves the keyboard to the explorer (showing it if it was hidden), or
    /// back to the editor.
    pub(super) fn toggle_explorer_focus(&mut self) {
        if self.state.focus() == Focus::Explorer {
            self.state.set_focus(Focus::Editor);
        } else if self.state.layout().contains(&ComponentKind::Explorer) {
            self.state.show_explorer();
            self.state.set_focus(Focus::Explorer);
        } else {
            self.state.notify("the layout has no explorer");
        }
    }

    /// Hides the explorer, or shows it and gives it the keyboard. A hidden
    /// explorer cannot have the keyboard.
    pub(super) fn toggle_explorer(&mut self) {
        if !self.state.layout().contains(&ComponentKind::Explorer) {
            self.state.notify("the layout has no explorer");
            return;
        }
        match self.state.is_showing(&ComponentKind::Explorer) {
            true => {
                self.state.hide_explorer();
                if self.state.focus() == Focus::Explorer {
                    self.state.set_focus(Focus::Editor);
                }
            }
            false => {
                self.state.show_explorer();
                self.state.set_focus(Focus::Explorer);
            }
        }
    }

    /// Shows the active tab's file in the explorer, selects it and gives the
    /// explorer the keyboard.
    pub(super) fn reveal_active_file(&mut self) {
        let Some(path) = self.state.editor().buffer().path().map(Path::to_path_buf) else {
            self.state.notify("this file has no path yet");
            return;
        };
        if !self.state.layout().contains(&ComponentKind::Explorer) {
            self.state.notify("the layout has no explorer");
            return;
        }
        self.state.show_explorer();

        // The path may be spelled relative to where the editor was started.
        let listed =
            |explorer: &Explorer, path: &Path| explorer.rows().iter().any(|row| row.path == path);
        let mut shown = path.clone();
        if let Err(error) = self.state.explorer_mut().reveal(&shown) {
            self.state.notify_error(error.to_string());
        }
        if !listed(self.state.explorer(), &shown) {
            shown = canonical(&path);
            if let Err(error) = self.state.explorer_mut().reveal(&shown) {
                self.state.notify_error(error.to_string());
            }
        }
        if !listed(self.state.explorer(), &shown) {
            self.state
                .notify(format!("{} is not in the project", path.display()));
            return;
        }
        self.state.set_focus(Focus::Explorer);
    }

    pub(super) fn toggle_plugin_view(&mut self, id: &str) {
        if !self.state.toggle_plugin_view(&PluginViewId::new(id)) {
            self.state.notify(format!("no plugin view named {id}"));
        }
    }

    pub(super) fn explorer_command(&mut self, command: ExplorerCommand) -> IdeResult {
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

    /// The button came up after a press on an explorer row: a click opens what
    /// is there, a drag moves the row into the folder it was dropped on.
    fn release_explorer(&mut self, column: u16, row: u16) {
        let released = self.state.explorer_mut().release(column, row);
        let result = match released {
            Ok(Release::Nothing) => Ok(()),
            Ok(Release::Open(path)) => self.open_from_explorer(Some(path)),
            Ok(Release::Move { from, into }) => {
                self.move_by_drag(&from, &into);
                Ok(())
            }
            Err(error) => Err(error.into()),
        };
        if let Err(error) = result {
            self.state.notify_error(error.to_string());
        }
    }

    /// Mouse input for the explorer. Returns whether it was used up.
    pub(super) fn handle_explorer_mouse(&mut self, event: MouseEvent) -> bool {
        let (column, row) = (event.column, event.row);
        let over = self.state.is_showing(&ComponentKind::Explorer)
            && self.state.explorer().contains(column, row);
        match event.kind {
            // A press only selects; what a click does waits for the release, so a
            // press that becomes a drag opens nothing.
            MouseEventKind::Down(MouseButton::Left) if over => {
                self.state.set_focus(Focus::Explorer);
                self.state.explorer_mut().press(column, row);
                true
            }
            MouseEventKind::Drag(MouseButton::Left) if self.state.explorer().is_pressed() => {
                self.state.explorer_mut().drag_to(column, row);
                true
            }
            MouseEventKind::Up(MouseButton::Left) if self.state.explorer().is_pressed() => {
                self.release_explorer(column, row);
                true
            }
            MouseEventKind::Down(MouseButton::Right) if over => {
                self.right_click_explorer(column, row);
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
}

/// Where `focus_toward` can send the keyboard.
#[derive(Debug, Clone, Copy)]
enum Target {
    Pane(PaneId),
    Explorer,
    Terminal,
    Errors,
}

impl App {
    /// Moves the keyboard to the nearest editor pane, the explorer or the
    /// terminal in `direction` from where it is, going by the screen areas of
    /// the last layout pass. Stays where it is when nothing is that way.
    pub(super) fn focus_toward(&mut self, direction: FocusDirection) {
        let focus = self.state.focus();
        let from = match focus {
            Focus::Editor => self.state.workspace().focused_area(),
            Focus::Explorer => self.state.explorer().area(),
            Focus::Terminal => self.state.terminal().area(),
            Focus::Errors => self.state.errors().area(),
            Focus::Plugin => self
                .state
                .focused_plugin()
                .and_then(|id| self.state.plugin_area(id))
                .unwrap_or_default(),
        };

        let mut best = self
            .state
            .workspace()
            .nearest_pane(from, direction)
            .map(|(id, score)| (Target::Pane(id), score));
        let mut consider = |target: Target, area: Rect| {
            let Some(score) = neighbor_score(from, area, direction) else {
                return;
            };
            if area.width > 0 && area.height > 0 && best.is_none_or(|(_, nearest)| score < nearest)
            {
                best = Some((target, score));
            }
        };
        if focus != Focus::Explorer && self.state.is_showing(&ComponentKind::Explorer) {
            consider(Target::Explorer, self.state.explorer().area());
        }
        let shell_there = self.state.terminal().tab_count() > 0;
        if focus != Focus::Terminal
            && shell_there
            && self.state.is_showing(&ComponentKind::Terminal)
        {
            consider(Target::Terminal, self.state.terminal().area());
        }

        if focus != Focus::Errors && self.state.is_showing(&ComponentKind::Errors) {
            consider(Target::Errors, self.state.errors().area());
        }

        match best {
            Some((Target::Pane(id), _)) => {
                self.state.workspace_mut().focus_pane(id);
                self.state.set_focus(Focus::Editor);
            }
            Some((Target::Explorer, _)) => self.state.set_focus(Focus::Explorer),
            Some((Target::Terminal, _)) => self.state.set_focus(Focus::Terminal),
            Some((Target::Errors, _)) => self.state.set_focus(Focus::Errors),
            None => {}
        }
    }
}
