//! The views of a bar: switching the one it shows, by click or by key.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;

use super::App;
use crate::app::state::Focus;
use crate::ui::ComponentKind;
use crate::ui::bar;

impl App {
    /// A click on a tab of a bar shows that view and gives it the keyboard; a
    /// middle click goes on to the bar's next view. Returns whether the event was
    /// on a tab.
    pub(super) fn handle_bar_tab_mouse(&mut self, event: MouseEvent) -> bool {
        let step = match event.kind {
            MouseEventKind::Down(MouseButton::Left) => None,
            MouseEventKind::Down(MouseButton::Middle) => Some(1),
            _ => return false,
        };
        if self.modal_open() {
            return false;
        }
        let at = Position::new(event.column, event.row);
        let hit = self
            .state
            .placements(self.state.screen())
            .into_iter()
            .find_map(|placement| {
                let bars = placement.bar.clone()?;
                let (_, index) = bar::tab_rects(&placement)
                    .into_iter()
                    .find(|(rect, _)| rect.contains(at))?;
                let target = match step {
                    None => index,
                    Some(step) => (bars.active + step) % bars.views.len(),
                };
                Some(bars.views[target].clone())
            });
        let Some(view) = hit else {
            return false;
        };
        self.show_bar_view(&view);
        true
    }

    /// Shows the next (`1`) or previous (`-1`) view of the bar the keyboard is
    /// in, and gives it the keyboard.
    pub(super) fn cycle_bar_view(&mut self, step: isize) {
        let current = match self.state.focus() {
            Focus::Explorer => ComponentKind::Explorer,
            Focus::Terminal => ComponentKind::Terminal,
            Focus::Errors => ComponentKind::Errors,
            Focus::Editor => {
                self.state.notify("the editor is not in a bar");
                return;
            }
        };
        let Some(views) = self.state.layout().bar_views(&current).map(<[_]>::to_vec) else {
            self.state.notify("this pane is not in a bar");
            return;
        };
        if views.len() < 2 {
            self.state.notify("this bar has one view");
            return;
        }
        let at = views.iter().position(|view| *view == current).unwrap_or(0);
        let next = (at as isize + step).rem_euclid(views.len() as isize) as usize;
        self.show_bar_view(&views[next]);
    }

    /// Makes the bar of `view` show it, shows the bar and gives `view` the
    /// keyboard (a plugin view has none, so it goes back to the editor).
    pub(super) fn show_bar_view(&mut self, view: &ComponentKind) {
        match view {
            ComponentKind::Explorer => self.focus_explorer_shown(),
            ComponentKind::Terminal => self.show_terminal_with_keyboard(),
            ComponentKind::Errors => self.show_errors_with_keyboard(),
            ComponentKind::Plugin(_)
            | ComponentKind::Editor
            | ComponentKind::StatusBar
            | ComponentKind::MenuBar => {
                self.state.set_shown(view, true);
                self.state.set_focus(Focus::Editor);
            }
        }
    }

    /// Runs an action as if a key had asked for it.
    #[cfg(test)]
    pub(crate) fn do_action(&mut self, action: crate::event::action::Action) {
        self.run_action(action);
    }
}
