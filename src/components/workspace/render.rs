use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::components::editor::{EditorRef, display_name, render as editor_render};
use crate::state::AppState;
use crate::ui::Render;
use crate::ui::layout::Placement;
use crate::ui::pane_frame::PaneFrame;
use crate::ui::theme::Theme;

use super::Workspace;

impl Render for Workspace {
    /// Lays the panes out inside the placement, then lets each pane's active
    /// editor note its screen area and scroll to its cursor.
    fn prepare(&mut self, placement: &Placement) {
        for (id, area) in self.tree.layout(placement.area) {
            if let Some(index) = self.panes.iter().position(|pane| pane.id == id) {
                self.layout_pane(index, area, &placement.frame);
            }
        }
    }

    fn render(&self, frame: &mut Frame, state: &AppState, placement: &Placement) {
        let theme = state.theme();
        // Blank the area so closed panes leave nothing behind.
        frame.render_widget(Paragraph::new(""), placement.area);

        for pane in &self.panes {
            let bar_style = theme.style("tab.bar");
            frame.render_widget(Paragraph::new("").style(bar_style), pane.tab_bar);
            for (index, (tab, rect)) in pane.tabs.iter().zip(&pane.tab_rects).enumerate() {
                if rect.width == 0 {
                    continue;
                }

                let name = display_name(&self.documents[&tab.document].buffer);
                let label = truncate(&format!(" {name} "), usize::from(rect.width));
                let slot = if index == pane.active {
                    "tab.active"
                } else {
                    "tab.inactive"
                };

                frame.render_widget(Paragraph::new(label).style(theme.style(slot)), *rect);
            }

            let focused = pane.id == self.focused;
            let Some(tab) = pane.tabs.get(pane.active) else {
                draw_empty(frame, theme, pane.editor_area, &placement.frame, focused);
                continue;
            };

            let editor = EditorRef::new(&self.documents[&tab.document].buffer, &tab.view);
            editor_render::draw(
                frame,
                theme,
                editor,
                pane.editor_area,
                &placement.frame,
                focused,
                state.find().filter(|_| focused),
            );

            if let Some(find) = state.find().filter(|_| focused) {
                find.render(frame, theme, pane.find_bar);
            }
        }
    }
}

impl Workspace {
    fn layout_pane(&mut self, index: usize, area: Rect, frame: &PaneFrame) {
        let labels: Vec<String> = self.panes[index]
            .tabs
            .iter()
            .map(|tab| format!(" {} ", display_name(&self.documents[&tab.document].buffer)))
            .collect();

        let reserve_find_bar = self.find_bar && self.panes[index].id == self.focused;
        let pane = &mut self.panes[index];
        pane.area = area;
        pane.tab_bar = Rect {
            height: area.height.min(1),
            ..area
        };
        let body = Rect {
            y: area.y + pane.tab_bar.height,
            height: area.height - pane.tab_bar.height,
            ..area
        };
        (pane.editor_area, pane.find_bar) = match reserve_find_bar && body.height > 1 {
            true => (
                Rect {
                    height: body.height - 1,
                    ..body
                },
                Rect {
                    y: body.bottom() - 1,
                    height: 1,
                    ..body
                },
            ),
            false => (body, Rect::default()),
        };
        pane.tab_rects = tab_rects(&labels, pane.active, pane.tab_bar);

        let Some(tab) = pane.tabs.get_mut(pane.active) else {
            return;
        };
        let buffer = &self.documents[&tab.document].buffer;
        editor_render::prepare(buffer, &mut tab.view, pane.editor_area, frame);
    }
}

/// What an editor area with no open file shows.
pub(super) fn draw_empty(
    frame: &mut Frame,
    theme: &Theme,
    area: Rect,
    pane_frame: &PaneFrame,
    focused: bool,
) {
    let block = pane_frame.block(theme, pane_frame.title_text("Editor"), focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(" No open files. Press ctrl+n for a new one.")
            .style(theme.style("tab.inactive")),
        inner,
    );
}

/// Where each tab goes in the tab bar. Tabs scroll off the left so that the
/// active one is visible; tabs that do not fit get an empty rect.
pub(super) fn tab_rects(labels: &[String], active: usize, bar: Rect) -> Vec<Rect> {
    let widths: Vec<u16> = labels
        .iter()
        .map(|label| u16::try_from(label.width()).unwrap_or(u16::MAX))
        .collect();

    let mut first = 0;
    while first < active
        && widths[first..=active]
            .iter()
            .map(|w| u32::from(*w))
            .sum::<u32>()
            > u32::from(bar.width)
    {
        first += 1;
    }

    let end = bar.x + bar.width;
    let mut x = bar.x;
    let mut rects = Vec::with_capacity(labels.len());
    for (index, width) in widths.iter().enumerate() {
        if index < first || x >= end {
            rects.push(Rect { x, width: 0, ..bar });
            continue;
        }
        let width = (*width).min(end - x);
        rects.push(Rect { x, width, ..bar });
        x += width;
    }
    rects
}

/// `text` cut to at most `width` display columns.
pub(super) fn truncate(text: &str, width: usize) -> String {
    let mut used = 0;
    let mut out = String::new();
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > width {
            break;
        }
        used += w;
        out.push(c);
    }
    out
}
