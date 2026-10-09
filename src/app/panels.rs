//! The explorer and the other panels that can be shown, hidden or given the
//! keyboard.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

use super::App;
use crate::app::state::Focus;
use crate::components::explorer::ExplorerCommand;
use crate::components::{ComponentKind, PluginViewId};
use crate::error::IdeResult;

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
        match self.state.is_visible(&ComponentKind::Explorer) {
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

    /// Mouse input for the explorer. Returns whether it was used up.
    pub(super) fn handle_explorer_mouse(&mut self, event: MouseEvent) -> bool {
        let (column, row) = (event.column, event.row);
        let over = self.state.is_visible(&ComponentKind::Explorer)
            && self.state.explorer().contains(column, row);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) if over => {
                self.state.set_focus(Focus::Explorer);
                let chosen = self.state.explorer_mut().click(column, row);
                let result = match chosen {
                    Ok(chosen) => self.open_from_explorer(chosen),
                    Err(error) => Err(error.into()),
                };
                if let Err(error) = result {
                    self.state.notify_error(error.to_string());
                }
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
