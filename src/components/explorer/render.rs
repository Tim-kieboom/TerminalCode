use ratatui::Frame;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{Explorer, NodeKind, Row};
use crate::app::state::{AppState, Focus};
use crate::ui::Render;
use crate::ui::layout::Placement;
use crate::ui::theme::Theme;

const EMPTY_MESSAGE: &str = "no folder open";
/// A vertical line under the marker of each ancestor, padded to one level.
const GUIDE: &str = "│ ";

impl Render for Explorer {
    fn prepare(&mut self, placement: &Placement) {
        self.area = placement.area;
        self.body = placement.frame.inner(placement.area);
        self.scroll = self.scroll.min(self.max_scroll());
        if !self.follow_selection {
            return;
        }

        let height = usize::from(self.body.height);

        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if height > 0 && self.selected >= self.scroll + height {
            self.scroll = self.selected + 1 - height;
        }
    }

    fn render(&self, frame: &mut Frame, state: &AppState, placement: &Placement) {
        let theme = state.theme();
        let focused = state.focus() == Focus::Explorer;
        let title = placement.frame.title_text(placement.kind.title());
        let block = placement.frame.block(theme, title, focused);
        let inner = block.inner(placement.area);
        frame.render_widget(block, placement.area);

        if !self.has_project() {
            let style = theme.style("explorer.hint");
            frame.render_widget(Paragraph::new(EMPTY_MESSAGE).style(style), inner);
            return;
        }

        let active_path = state.editor().buffer().path();
        let drop_target = self.drop_target();
        let lines: Vec<Line> = self
            .rows()
            .iter()
            .enumerate()
            .skip(self.scroll)
            .take(usize::from(inner.height))
            .map(|(index, row)| {
                let active = active_path.is_some_and(|path| path == row.path);
                let drop = drop_target.is_some_and(|path| path == row.path);
                line(row, index == self.selected, active, drop, focused, theme)
            })
            .collect();

        frame.render_widget(Paragraph::new(lines), inner);
    }
}

fn line<'a>(
    row: &Row,
    selected: bool,
    active: bool,
    drop: bool,
    focused: bool,
    theme: &Theme,
) -> Line<'a> {
    let marker = match (row.kind, row.expanded) {
        (NodeKind::Dir, _) if row.looped => "↻ ",
        (NodeKind::Dir, true) => "▾ ",
        (NodeKind::Dir, false) => "▸ ",
        (NodeKind::File, _) => "  ",
    };
    let guides = GUIDE.repeat(row.depth);
    let text = format!("{marker}{}", row.name);
    let kind_slot = match row.kind {
        NodeKind::Dir => "explorer.dir",
        NodeKind::File => "explorer.file",
    };
    // The file open in the editor stands out from the rest.
    let mut name_style = theme.style(kind_slot);
    // Left out by the ignore rules: dimmer than the rest.
    if row.ignored {
        name_style = name_style.patch(theme.style("explorer.ignored"));
    }
    if active {
        name_style = name_style.patch(theme.style("explorer.active"));
    }
    let line = Line::from(vec![
        Span::styled(guides, theme.style("explorer.guide")),
        Span::styled(text, name_style),
    ]);
    // The folder a dragged row would go into.
    if drop {
        return line.style(theme.style("explorer.drop"));
    }
    if !selected {
        return line;
    }
    let slot = match focused {
        true => "explorer.selected",
        false => "explorer.selected.inactive",
    };
    // A line style covers the whole row, not just the text.
    line.style(theme.style(slot))
}
