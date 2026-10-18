use ratatui::Frame;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use super::{Explorer, NodeKind, Row};
use crate::app::state::{AppState, Focus};
use crate::files::paths::{canonical, without_verbatim_prefix};
use crate::git::Mark;
use crate::ui::Render;
use crate::ui::layout::Placement;
use crate::ui::theme::Theme;

const EMPTY_MESSAGE: &str = "no folder open";
/// A vertical line under the marker of each ancestor, padded to one level.
const GUIDE: &str = "│ ";

impl Render<AppState> for Explorer {
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
        let live = state.has_keyboard(Focus::Explorer);
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
        let resolve = self.resolver();
        let lines: Vec<Line> = self
            .rows()
            .iter()
            .enumerate()
            .skip(self.scroll)
            .take(usize::from(inner.height))
            .map(|(index, row)| {
                let active = active_path.is_some_and(|path| path == row.path);
                let drop = drop_target.is_some_and(|path| path == row.path);
                let mark = state.git_mark(&resolve.path_of(&row.path));
                let shown = Shown {
                    selected: index == self.selected,
                    active,
                    drop,
                    focused: live,
                    mark,
                    width: inner.width,
                };
                line(row, &shown, theme)
            })
            .collect();

        frame.render_widget(Paragraph::new(lines), inner);
    }
}

/// Where the explorer's paths really are, for asking git about them: the
/// project may be spelled with a link or a relative path, which git does not
/// use. Worked out once for each frame, not for each row.
struct Resolver<'a> {
    spelled: &'a std::path::Path,
    real: std::path::PathBuf,
}

impl Explorer {
    fn resolver(&self) -> Resolver<'_> {
        let spelled = self.root_path().unwrap_or(std::path::Path::new(""));
        Resolver {
            spelled,
            real: without_verbatim_prefix(&canonical(spelled)),
        }
    }
}

impl Resolver<'_> {
    fn path_of(&self, path: &std::path::Path) -> std::path::PathBuf {
        match path.strip_prefix(self.spelled) {
            Ok(inside) => self.real.join(inside),
            Err(_) => path.to_path_buf(),
        }
    }
}

/// How one row is drawn, apart from the row itself.
struct Shown {
    selected: bool,
    /// The file open in the editor.
    active: bool,
    /// The folder a dragged row would go into.
    drop: bool,
    focused: bool,
    /// What git says of the file, or of a folder with such files inside.
    mark: Option<Mark>,
    /// The width of the row.
    width: u16,
}

fn line<'a>(row: &Row, shown: &Shown, theme: &Theme) -> Line<'a> {
    let Shown {
        selected,
        active,
        drop,
        focused,
        mark,
        width,
    } = *shown;
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
    if let Some(mark) = mark {
        name_style = name_style.patch(theme.style(mark.slot()));
    }
    let used = guides.width() + text.width();
    let mut spans = vec![
        Span::styled(guides, theme.style("explorer.guide")),
        Span::styled(text, name_style),
    ];
    // The letter sits at the right edge, when the name leaves room for it; a
    // folder has the color only.
    let letter = mark.filter(|_| row.kind == NodeKind::File);
    if let Some(mark) = letter.filter(|_| used + 2 <= usize::from(width)) {
        let gap = usize::from(width) - used - 1;
        spans.push(Span::styled(
            format!("{:gap$}{}", "", mark.letter),
            theme.style(mark.slot()),
        ));
    }
    let line = Line::from(spans);
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
