//! Paths in the output of the terminal pane: with `ctrl` held the path under the pointer is
//! underlined, and a click on it opens the file there (a folder is shown in the explorer). Only
//! what is a file or a folder of the project is a link; a relative path is the project's.

use std::path::PathBuf;

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use super::App;
use crate::components::terminal::{Link, LinkText};

impl App {
    /// The pointer moves over the terminal's text, or presses: with `ctrl` the path under it is
    /// underlined, and a left press on it opens it. Returns whether the event was used up.
    /// A program that asked for the mouse keeps it, unless `shift` is held.
    pub(super) fn terminal_link_mouse(&mut self, event: MouseEvent) -> bool {
        let wants_link = event.modifiers.contains(KeyModifiers::CONTROL)
            && !self.program_has_the_mouse(event.modifiers);
        let link = match wants_link {
            true => self.terminal_link_under(event.column, event.row),
            false => None,
        };
        let cells = link.as_ref().map(|(link, _)| link.cells.clone());
        if self.state.terminal_mut().set_link(cells) {
            self.needs_redraw = true;
        }
        let pressed = matches!(event.kind, MouseEventKind::Down(MouseButton::Left));
        let (Some((link, target)), true) = (link, pressed) else {
            return false;
        };
        self.open_terminal_link(&link.text, target);
        true
    }

    /// Whether the program in the tab has asked for the mouse, and `shift` is not held (which is
    /// how that program is told to leave it).
    fn program_has_the_mouse(&self, modifiers: KeyModifiers) -> bool {
        if modifiers.contains(KeyModifiers::SHIFT) {
            return false;
        }
        self.state.terminal().session().is_some_and(|session| {
            session.with_screen(|screen| {
                screen.mouse_protocol_mode() != vt100::MouseProtocolMode::None
            })
        })
    }

    /// The path under a screen cell, when it names a file or a folder that is there, and where
    /// it is.
    fn terminal_link_under(&self, column: u16, row: u16) -> Option<(Link, PathBuf)> {
        let cell = self.state.terminal().cell_at(column, row)?;
        let link = self.state.terminal().link_at(cell)?;
        let target = self.resolve_terminal_path(&link.text.path)?;
        Some((link, target))
    }

    /// The file or folder a path of the terminal's output is: as it is when it is whole, else
    /// from the project folder. `None` when nothing is there.
    fn resolve_terminal_path(&self, path: &str) -> Option<PathBuf> {
        let given = PathBuf::from(path);
        let full = match given.is_absolute() {
            true => given,
            false => self.project_root().join(given),
        };
        full.exists().then_some(full)
    }

    /// Opens the file at the line and column of the link, or shows the folder in the explorer.
    fn open_terminal_link(&mut self, text: &LinkText, target: PathBuf) {
        if target.is_dir() {
            self.reveal_in_explorer(target);
            return;
        }
        if let Some(here) = self.here() {
            self.record_place(here);
        }
        if let Err(error) = self.state.workspace_mut().open_path(&target) {
            self.state.notify_error(error.to_string());
            return;
        }
        let line = text.line.unwrap_or(1);
        let column = text.column.unwrap_or(1);
        self.show_place(
            u32::try_from(line).unwrap_or(u32::MAX),
            u32::try_from(column).unwrap_or(u32::MAX),
        );
        self.needs_redraw = true;
    }
}
