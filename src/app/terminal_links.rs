//! Paths in the output of the terminal pane: with `ctrl` held the path under the pointer is
//! underlined, and a click on it opens the file there (a folder is shown in the explorer). Only
//! what is a file or a folder of the project is a link; a relative path is the project's.

use std::path::PathBuf;

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use ratatui::layout::Position;

use super::App;
use crate::components::popups::menu::{Entry, Menu};
use crate::components::terminal::{Link, LinkText, split_place};
use crate::event::action::Action;

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
    /// from the project folder, else from the folder the shell is in (where the platform says).
    /// `None` when nothing is there.
    fn resolve_terminal_path(&self, path: &str) -> Option<PathBuf> {
        let shell_folder = self
            .state
            .terminal()
            .session()
            .and_then(|session| session.working_folder());
        let bases = [Some(self.project_root()), shell_folder];
        first_that_exists(path, bases.into_iter().flatten())
    }

    /// A right click on a path of the output: a menu to open it or copy it. Returns whether
    /// there was a path under the pointer. A program that asked for the mouse keeps the click,
    /// unless `shift` is held.
    pub(super) fn terminal_link_menu(&mut self, event: MouseEvent) -> bool {
        if self.program_has_the_mouse(event.modifiers) {
            return false;
        }
        let Some((link, _)) = self.terminal_link_under(event.column, event.row) else {
            return false;
        };
        let written = written_text(&link.text);
        let item = |label: String, action: Action| Entry::Item {
            label: label.into(),
            action,
            keys: None,
        };
        let entries = vec![
            item(
                format!("Open {}", link.text.path),
                Action::OpenTerminalPath(written.clone().into()),
            ),
            item(
                "Copy Path".to_owned(),
                Action::CopyTerminalPath(written.into()),
            ),
        ];
        self.state.open_menu(Menu::on_app(
            entries,
            Position::new(event.column, event.row),
        ));
        self.needs_redraw = true;
        true
    }

    /// Opens the path of the menu, written as the output had it (`src/main.rs:3:5`).
    pub(super) fn open_terminal_path(&mut self, written: &str) {
        let (path, line, column) = split_place(written);
        let Some(target) = self.resolve_terminal_path(path) else {
            self.state
                .notify_error(format!("{path} is no longer there"));
            return;
        };
        let text = LinkText {
            path: path.to_owned(),
            line,
            column,
            start: 0,
            end: 0,
        };
        self.open_terminal_link(&text, target);
    }

    /// Copies the whole path of the menu.
    pub(super) fn copy_terminal_path(&mut self, written: &str) {
        let (path, _, _) = split_place(written);
        let Some(target) = self.resolve_terminal_path(path) else {
            self.state
                .notify_error(format!("{path} is no longer there"));
            return;
        };
        self.copy_path_text(&target, false);
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

/// The path from the first of `bases` it is there from (a whole path is its own): the file or
/// folder, when there is one.
fn first_that_exists(path: &str, bases: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    let given = PathBuf::from(path);
    if given.is_absolute() {
        return given.exists().then_some(given);
    }
    bases
        .into_iter()
        .map(|base| base.join(&given))
        .find(|full| full.exists())
}

/// The link as the output had it: the path, then the line and the column.
fn written_text(link: &LinkText) -> String {
    match (link.line, link.column) {
        (Some(line), Some(column)) => format!("{}:{line}:{column}", link.path),
        (Some(line), None) => format!("{}:{line}", link.path),
        _ => link.path.clone(),
    }
}

#[cfg(test)]
mod tests;
