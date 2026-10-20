//! Moving the lines between panes: the one between the editor and the pane right of it (a
//! plugin's preview), and the ones between editor panes that were split. Drag the border
//! between two with the mouse, or use the commands of the palette. The side pane's width is
//! remembered in `state.json`; the split of editor panes is not (the panes are not either).

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::App;
use crate::components::workspace::Divider;
use crate::ui::ComponentKind;

/// A line the mouse holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Dragging {
    /// The line between the editor and the side pane.
    Side,
    /// The line between two editor panes.
    Panes(Divider),
}

impl App {
    /// The area of the editor and of the pane right of it, when the user's layout does not
    /// have one of its own (the width of that one is in the layout file).
    fn side_geometry(&self) -> Option<(Rect, Rect)> {
        if self.state.base_layout().right_bar_anchor().is_some() {
            return None;
        }
        let layout = self.state.layout();
        let views = layout.bar_views(&layout.right_bar_anchor()?)?;
        let placements = self.state.placements(self.state.screen());
        let bar = placements.iter().find(|p| views.contains(&p.kind))?.area;
        let editor = placements
            .iter()
            .find(|p| p.kind == ComponentKind::Editor)?
            .area;
        (editor.right() == bar.x).then_some((editor, bar))
    }

    /// The line the pointer is on, if it is on one.
    fn divider_under(&self, column: u16, row: u16) -> Option<Dragging> {
        if let Some((_, bar)) = self.side_geometry()
            && (column + 1 == bar.x || column == bar.x)
            && row >= bar.y
            && row < bar.bottom()
        {
            return Some(Dragging::Side);
        }
        let placements = self.state.placements(self.state.screen());
        let editor = placements
            .iter()
            .find(|p| p.kind == ComponentKind::Editor)?;
        self.state
            .workspace()
            .divider_at(editor.area, column, row)
            .map(Dragging::Panes)
    }

    /// A press on a line takes hold of it, a drag moves it and the release lets go.
    /// Returns whether the event was about a line.
    pub(super) fn handle_divider_mouse(&mut self, event: MouseEvent) -> bool {
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if self.modal_open() {
                    return false;
                }
                self.dragging = self.divider_under(event.column, event.row);
                self.dragging.is_some()
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let Some(held) = self.dragging.clone() else {
                    return false;
                };
                match held {
                    Dragging::Side => {
                        if let Some((editor, bar)) = self.side_geometry() {
                            let whole = bar.right().saturating_sub(editor.x).max(1);
                            let width = bar.right().saturating_sub(event.column);
                            let percent = (u32::from(width) * 100 / u32::from(whole)) as u16;
                            self.state.set_side_percent(percent);
                        }
                    }
                    Dragging::Panes(divider) => {
                        self.state
                            .workspace_mut()
                            .drag_divider(&divider, event.column, event.row);
                    }
                }
                true
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let Some(held) = self.dragging.take() else {
                    return false;
                };
                if held == Dragging::Side {
                    self.remember_side_percent(self.state.side_percent());
                }
                true
            }
            _ => false,
        }
    }

    /// Gives the side pane `step` percent more of the editor's row, or less.
    pub(super) fn resize_side_pane(&mut self, step: i16) {
        if self.side_geometry().is_none() {
            self.state.notify("there is no pane right of the editor");
            return;
        }
        let percent = i32::from(self.state.side_percent()) + i32::from(step);
        self.state.set_side_percent(percent.clamp(0, 100) as u16);
        self.remember_side_percent(self.state.side_percent());
        self.needs_redraw = true;
    }

    /// Gives the focused editor pane `step` thousandths more of its split, or less.
    pub(super) fn resize_focused_pane(&mut self, step: i16) {
        if !self.state.workspace_mut().resize_focused(step) {
            self.state.notify("there is only one pane: split it first");
        }
        self.needs_redraw = true;
    }
}
