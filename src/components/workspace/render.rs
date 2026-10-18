use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::state::{AppState, Focus};
use crate::components::editor::{EditorRef, display_name, render as editor_render};
use crate::ui::Render;
use crate::ui::layout::Placement;
use crate::ui::pane_frame::PaneFrame;
use crate::ui::theme::Theme;

use super::Workspace;

impl Render<AppState> for Workspace {
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
            let (before, after) = pane.hidden;
            let overflow_style = theme.style("tab.overflow");
            if before > 0 {
                frame.render_widget(
                    Paragraph::new(format!("‹{before}")).style(overflow_style),
                    pane.overflow.0,
                );
            }
            if after > 0 {
                frame.render_widget(
                    Paragraph::new(format!("{after}›")).style(overflow_style),
                    pane.overflow.1,
                );
            }
            for (index, (tab, rect)) in pane.tabs.iter().zip(&pane.tab_rects).enumerate() {
                if rect.width == 0 {
                    continue;
                }

                let pin = if tab.pinned { PIN } else { "" };
                let label = truncate(
                    &format!(" {pin}{} ", self.tab_title(tab.document)),
                    usize::from(rect.width),
                );
                let slot = if index == pane.active {
                    "tab.active"
                } else {
                    "tab.inactive"
                };

                let mut style = theme.style(slot);
                let path = self.documents[&tab.document].buffer.path();
                if let Some(mark) = path.and_then(|path| state.git_mark(path)) {
                    style = style.patch(theme.style(mark.slot()));
                }

                frame.render_widget(Paragraph::new(label).style(style), *rect);
            }

            let focused = pane.id == self.focused;
            let Some(tab) = pane.tabs.get(pane.active) else {
                draw_empty(frame, theme, pane.editor_area, &placement.frame, focused);
                continue;
            };

            let document = &self.documents[&tab.document];
            let editor = EditorRef::new(&document.buffer, &tab.view);
            let content = editor_render::Content {
                editor,
                syntax: document.syntax.spans(),
                focused,
                live: focused && state.has_keyboard(Focus::Editor),
                find: state.find().filter(|_| focused),
                diagnostics: state.diagnostics().of(tab.document),
                git: &document.git,
            };
            editor_render::draw(frame, theme, content, pane.editor_area, &placement.frame);

            if let Some(find) = state.find().filter(|_| focused) {
                find.render(frame, theme, pane.find_bar);
            }
        }
    }
}

/// Written before the name of a pinned tab.
const PIN: &str = "◆ ";

impl Workspace {
    /// What a tab says: the file name, a mark for unsaved changes and a note
    /// when the file changed on disk under them.
    fn tab_title(&self, document: super::DocumentId) -> String {
        let doc = &self.documents[&document];
        let read_only = if doc.read_only { " [read-only]" } else { "" };
        format!(
            "{}{read_only}{}",
            display_name(&doc.buffer),
            self.disk_note(document)
        )
    }

    fn layout_pane(&mut self, index: usize, area: Rect, frame: &PaneFrame) {
        let labels: Vec<String> = self.panes[index]
            .tabs
            .iter()
            .map(|tab| {
                let pin = if tab.pinned { PIN } else { "" };
                format!(" {pin}{} ", self.tab_title(tab.document))
            })
            .collect();

        let find_rows = match self.panes[index].id == self.focused {
            true => self.find_bar,
            false => 0,
        };
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
        // The bar leaves at least one row of text.
        let rows = find_rows.min(body.height.saturating_sub(1));
        (pane.editor_area, pane.find_bar) = match rows > 0 {
            true => (
                Rect {
                    height: body.height - rows,
                    ..body
                },
                Rect {
                    y: body.bottom() - rows,
                    height: rows,
                    ..body
                },
            ),
            false => (body, Rect::default()),
        };
        let bar = tab_bar_layout(&labels, pane.active, pane.tab_bar);
        pane.tab_rects = bar.rects;
        pane.overflow = (bar.before, bar.after);
        pane.hidden = (bar.hidden_before, bar.hidden_after);

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

/// The tab bar of one pane: where each tab goes, and the markers for the tabs
/// that scrolled out of it.
pub(super) struct TabBar {
    pub(super) rects: Vec<Rect>,
    /// How many tabs are out of view before the first visible one, and after
    /// the last.
    pub(super) hidden_before: usize,
    pub(super) hidden_after: usize,
    /// Where the markers go; empty when there is nothing hidden on that side.
    pub(super) before: Rect,
    pub(super) after: Rect,
}

/// The width of a marker that says "`hidden` tabs": an arrow and the number.
fn marker_width(hidden: usize) -> u16 {
    match hidden {
        0 => 0,
        n => 1 + n.to_string().len() as u16,
    }
}

/// Where the tabs go in the bar of a pane, with room kept at the ends of the bar
/// for the markers of the tabs that do not fit.
///
/// A run of tabs that holds the active one is shown: the longest that fits
/// with its markers, and of those the one that starts first, so that moving
/// to the next tab moves the run by one. A bar too narrow for even the active
/// tab shows it cut, with no markers.
pub(super) fn tab_bar_layout(labels: &[String], active: usize, bar: Rect) -> TabBar {
    let widths: Vec<u32> = labels
        .iter()
        .map(|label| u32::try_from(label.width()).unwrap_or(u32::MAX))
        .collect();
    let count = widths.len();
    let room = u32::from(bar.width);
    let hidden = || Rect { width: 0, ..bar };
    let tabs_only = |rects: Vec<Rect>| TabBar {
        rects,
        hidden_before: 0,
        hidden_after: 0,
        before: Rect::default(),
        after: Rect::default(),
    };
    if count == 0 || room == 0 {
        return tabs_only(vec![hidden(); count]);
    }
    let active = active.min(count - 1);
    let marker = |hidden_tabs: usize| u32::from(marker_width(hidden_tabs));

    // The (first, last) tabs of the longest run that fits, the earliest first.
    let mut best: Option<(usize, usize)> = None;
    for first in 0..=active {
        let mut used = marker(first) + widths[first..=active].iter().sum::<u32>();
        if used + marker(count - 1 - active) > room {
            continue;
        }
        // Take tabs on the right while the run and its markers still fit.
        let mut last = active;
        while last + 1 < count && used + widths[last + 1] + marker(count - 2 - last) <= room {
            used += widths[last + 1];
            last += 1;
        }
        if best.is_none_or(|(f, l)| last - first > l - f) {
            best = Some((first, last));
        }
    }

    let Some((first, last)) = best else {
        // Not even the active tab fits with its markers: show it, cut.
        let mut rects = vec![hidden(); count];
        rects[active] = Rect {
            width: u16::try_from(widths[active].min(room)).unwrap_or(bar.width),
            ..bar
        };
        return tabs_only(rects);
    };

    let (hidden_before, hidden_after) = (first, count - 1 - last);
    let (left, right) = (marker_width(hidden_before), marker_width(hidden_after));
    let mut x = bar.x + left;
    let rects = (0..count)
        .map(|index| {
            if index < first || index > last {
                return hidden();
            }
            let width = u16::try_from(widths[index]).unwrap_or(u16::MAX);
            let rect = Rect { x, width, ..bar };
            x += width;
            rect
        })
        .collect();
    TabBar {
        rects,
        hidden_before,
        hidden_after,
        before: Rect { width: left, ..bar },
        after: Rect {
            x: bar.right() - right,
            width: right,
            ..bar
        },
    }
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
