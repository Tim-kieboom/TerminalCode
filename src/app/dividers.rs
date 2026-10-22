//! Moving the lines between panes: the one between the editor and the pane right of it (a
//! plugin's preview), and the ones between editor panes that were split. Drag the border
//! between two with the mouse, or use the commands of the palette. The side pane's width is
//! remembered in `state.json`; the split of editor panes is not (the panes are not either).

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use super::App;
use crate::components::workspace::Divider;
use crate::ui::ComponentKind;
use crate::ui::layout::ResizeEdge;

/// A line the mouse holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Dragging {
    /// The line between the editor and the side pane.
    Side,
    /// The line between two editor panes.
    Panes(Divider),
    /// A selection of text in the terminal pane, which the pointer is extending.
    Terminal,
    /// The edge of the explorer's or the terminal's bar: `key` is the bar (its first view),
    /// `anchor` the far side that stays (the left edge, or the bottom) and `grab` how far the
    /// pointer was from the edge it took hold of, which the drag keeps.
    Bar {
        key: ComponentKind,
        edge: ResizeEdge,
        anchor: u16,
        grab: i32,
    },
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
        for placement in &placements {
            let (area, Some(edge)) = (placement.area, placement.resize_edge) else {
                continue;
            };
            let key = self
                .state
                .layout()
                .bar_views(&placement.kind)
                .and_then(|views| views.first().cloned())
                .unwrap_or_else(|| placement.kind.clone());
            // The bar's own last column or first row, which its border is on in the default
            // layout: not the editor's side, whose first cell can be text. A tab of the bar,
            // in its top row, is a tab and not a line.
            let on_tab = crate::ui::bar::tab_rects(placement)
                .iter()
                .any(|(rect, _)| rect.contains(ratatui::layout::Position::new(column, row)));
            let held = match edge {
                ResizeEdge::Right => {
                    column + 1 == area.right() && row >= area.y && row < area.bottom()
                }
                ResizeEdge::Top => {
                    row == area.y && column >= area.x && column < area.right() && !on_tab
                }
            };
            if held {
                return Some(match edge {
                    ResizeEdge::Right => Dragging::Bar {
                        key,
                        edge,
                        anchor: area.x,
                        grab: i32::from(area.right()) - i32::from(column),
                    },
                    ResizeEdge::Top => Dragging::Bar {
                        key,
                        edge,
                        anchor: area.bottom(),
                        grab: i32::from(area.y) - i32::from(row),
                    },
                });
            }
        }
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
                    Dragging::Terminal => self.extend_terminal_selection(event.column, event.row),
                    Dragging::Bar {
                        key,
                        edge,
                        anchor,
                        grab,
                    } => {
                        let cells = match edge {
                            ResizeEdge::Right => i32::from(event.column) + grab - i32::from(anchor),
                            ResizeEdge::Top => i32::from(anchor) - (i32::from(event.row) + grab),
                        };
                        let cells = cells.clamp(0, i32::from(u16::MAX)) as u16;
                        self.state.resize_bar(&key, cells);
                    }
                }
                true
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let Some(held) = self.dragging.take() else {
                    return false;
                };
                match held {
                    Dragging::Side => self.remember_side_percent(self.state.side_percent()),
                    // Letting go copies what was selected.
                    Dragging::Terminal => {
                        self.extend_terminal_selection(event.column, event.row);
                        self.copy_terminal_selection(false);
                    }
                    Dragging::Panes(_) | Dragging::Bar { .. } => {}
                }
                true
            }
            _ => false,
        }
    }

    /// Gives the explorer's and the terminal's bars the sizes the layout file says again.
    pub(super) fn reset_pane_sizes(&mut self) {
        self.state.reset_sizes();
        self.needs_redraw = true;
    }

    /// Shows the pane right of the editor, or hides it; the choice is remembered. The keyboard
    /// leaves it when it is hidden.
    pub(super) fn toggle_side_pane(&mut self) {
        if !self.state.has_side_pane() {
            self.state.notify(
                "there is no pane right of the editor: put a plugin there in Plugins: Manage",
            );
            return;
        }
        let shown = !self.state.side_pane_shown();
        if !shown && self.state.side_pane_has_keyboard() {
            self.state.set_focus(super::state::Focus::Editor);
        }
        self.state.set_side_pane_shown(shown);
        self.remember_side_shown(shown);
        self.needs_redraw = true;
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
