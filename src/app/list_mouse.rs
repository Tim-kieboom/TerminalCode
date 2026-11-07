//! The mouse in the file finder, the project search and the plugin list: the pointer over a row
//! selects it (as the keys would), a click on a row of the finder or the search does what `enter`
//! does, a click on a plugin selects it, the wheel moves the selection, and a click outside the box
//! closes it (as `esc` does). A press on the box that is not on a row does nothing. Like the
//! palette, each takes every mouse event while it is open.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use super::App;
use crate::app::state::Popup;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

impl App {
    /// The mouse while the file finder is open. Whether it was.
    pub(super) fn handle_finder_mouse(&mut self, event: MouseEvent) -> bool {
        let screen = self.state.screen();
        let Some(finder) = self.state.finder_mut() else {
            return false;
        };
        let (column, row) = (event.column, event.row);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) if finder.click_text(column, row) => {}
            MouseEventKind::Down(MouseButton::Left) => match finder.row_at(screen, column, row) {
                Some(index) => {
                    finder.select(index);
                    self.handle_finder_key(key(KeyCode::Enter));
                }
                None if !finder.contains(screen, column, row) => {
                    self.handle_finder_key(key(KeyCode::Esc));
                }
                None => {}
            },
            MouseEventKind::Moved => {
                if let Some(index) = finder.row_at(screen, column, row) {
                    finder.select(index);
                }
            }
            MouseEventKind::ScrollUp => finder.select_previous(),
            MouseEventKind::ScrollDown => finder.select_next(),
            _ => {}
        }
        self.needs_redraw = true;
        true
    }

    /// The mouse while the project search is open. Whether it was.
    pub(super) fn handle_search_mouse(&mut self, event: MouseEvent) -> bool {
        let screen = self.state.screen();
        let Some(search) = self.state.search_mut() else {
            return false;
        };
        let (column, row) = (event.column, event.row);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) if search.click_text(column, row) => {}
            MouseEventKind::Down(MouseButton::Left) => match search.row_at(screen, column, row) {
                Some(index) => {
                    search.select(index);
                    self.handle_search_key(key(KeyCode::Enter));
                }
                None if !search.contains(screen, column, row) => {
                    self.handle_search_key(key(KeyCode::Esc));
                }
                None => {}
            },
            MouseEventKind::Moved => {
                if let Some(index) = search.row_at(screen, column, row) {
                    search.select(index);
                }
            }
            MouseEventKind::ScrollUp => search.select_previous(),
            MouseEventKind::ScrollDown => search.select_next(),
            _ => {}
        }
        self.needs_redraw = true;
        true
    }

    /// The mouse while the plugin list is open: a click or the pointer over a plugin selects it
    /// (what to do with it is `enter` and the letters under the list), the wheel steps, and a
    /// click outside closes the list, unless something is being downloaded. Whether it was open.
    pub(super) fn handle_plugin_list_mouse(&mut self, event: MouseEvent) -> bool {
        if !matches!(self.state.popup(), Popup::PluginList(_)) {
            return false;
        }
        let screen = self.state.screen();
        let Some(list) = self.state.plugin_list_mut() else {
            return false;
        };
        let (column, row) = (event.column, event.row);
        let busy = list.is_busy();
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => match list.row_at(screen, column, row) {
                Some(index) => list.select_row(index),
                None if !list.contains(screen, column, row) && !busy => {
                    self.state.close_plugin_list();
                }
                None => {}
            },
            MouseEventKind::Moved => {
                if let Some(index) = list.row_at(screen, column, row) {
                    list.select_row(index);
                }
            }
            MouseEventKind::ScrollUp => list.step(false),
            MouseEventKind::ScrollDown => list.step(true),
            _ => {}
        }
        self.needs_redraw = true;
        true
    }
}
