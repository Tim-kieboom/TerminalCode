use std::path::PathBuf;

use crate::{
    StartupArgs,
    app::components::{
        Component,
        sidebar::file_tree::VisibleIndex,
        utils::cursor_scroller::{CursorScroller, Position, ScrollMode},
    },
    keybinds::{Action, PanelContext},
    theme::Theme,
};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::file_tree::FileTree;

pub struct Explorer {
    tree: FileTree,
    scroller: CursorScroller,
}

impl Explorer {
    pub fn new(args: &StartupArgs) -> Self {
        Self {
            tree: FileTree::new(args.project_path.clone()),
            scroller: CursorScroller::new(ScrollMode::List),
        }
    }

    pub fn move_cursor(&mut self, action: Action) {
        let length = self.tree.visible().len();
        self.scroller.move_cursor(action, length, 0);
    }

    pub fn open_current(&mut self) -> Option<PathBuf> {
        let visible = self.tree.visible();
        let index = self.scroller.cursor().vertical;

        let VisibleIndex { file, .. } = visible.get(index)?;

        let node_index = *file;
        let node = self.tree.node(node_index);
        if !node.is_dir() {
            return Some(node.path().to_path_buf());
        }

        self.tree.toggle(node_index);
        self.clamp_cursor();
        None
    }

    fn clamp_cursor(&mut self) {
        let length = self.tree.visible().len().saturating_sub(1);
        let vertical = self.scroller.cursor().vertical.min(length);
        let horizontal = self.scroller.cursor().horizontal;
        self.scroller.set_cursor(Position {
            vertical,
            horizontal,
        });
    }
}

impl Component for Explorer {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: PanelContext) {
        let focused = context == PanelContext::SideBar;
        let visible = self.tree.visible();
        let inner_height = area.height.saturating_sub(2);
        let cursor_visual_line = self.scroller.cursor().vertical as u16;
        let scroll_offset = self
            .scroller
            .get_scroll(cursor_visual_line, inner_height, 0);

        let mut lines: Vec<Line> = Vec::new();
        for (i, VisibleIndex { file, depth }) in visible.iter().enumerate() {
            let node = self.tree.node(*file);
            let selected = i == self.scroller.cursor().vertical;

            let indent = "  ".repeat(*depth);
            let icon = if node.is_dir() {
                if node.is_expanded() { "▾ " } else { "▸ " }
            } else {
                "  "
            };

            let mut dim = Theme::text_dim();
            let mut name = if node.is_dir() {
                Theme::explorer_folder()
            } else {
                Theme::explorer_file()
            };
            if selected {
                Theme::add_highlight(&mut dim);
                Theme::add_highlight(&mut name);
            }

            lines.push(Line::from(vec![
                Span::styled(format!("{indent}{icon}"), dim),
                Span::styled(node.name(), name),
            ]));
        }

        let title_style = if focused {
            Theme::title_focused()
        } else {
            Theme::title_default()
        };

        let border_style = if focused {
            Theme::border_focused()
        } else {
            Theme::border_default()
        };

        let block = Block::default()
            .title(Span::styled(" Explorer ", title_style))
            .borders(Borders::ALL)
            .border_style(border_style);

        let paragraph = Paragraph::new(lines)
            .block(block)
            .style(Theme::explorer_bg())
            .scroll((scroll_offset.vertical, scroll_offset.horizontal));

        frame.render_widget(paragraph, area);
    }
}
