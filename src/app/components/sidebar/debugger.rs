use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    StartupArgs,
    app::components::{
        Component,
        utils::cursor_scroller::{CursorScroller, ScrollMode},
    },
    keybinds::{Action, PanelContext},
    launch::{LaunchConfig, LaunchConfiguration},
    theme::Theme,
};

pub struct Debugger {
    scroller: CursorScroller,
    launch: Option<LaunchConfig>,
    error: Option<String>,
}

impl Debugger {
    pub fn new(args: &StartupArgs) -> Self {
        let (launch, error) = match LaunchConfig::load(args.project_path()) {
            Ok(Some(launch)) => (Some(launch), None),
            Ok(None) => (None, None),
            Err(err) => (None, Some(err.to_string())),
        };

        Self {
            scroller: CursorScroller::new(ScrollMode::List),
            launch,
            error,
        }
    }

    pub fn move_cursor(&mut self, action: Action) {
        let length = self.configurations().len();
        self.scroller.move_cursor(action, length, 0);
    }

    fn configurations(&self) -> &[LaunchConfiguration] {
        self.launch
            .as_ref()
            .map_or(&[], |launch| launch.configurations())
    }

    fn draw_content(&self, width: u16) -> (Vec<Line<'static>>, u16) {
        if let Some(error) = &self.error {
            let max = width.saturating_sub(4) as usize;
            let mut lines = vec![Line::from("")];
            lines.extend(
                wrapped_lines(error, "  ", max)
                    .into_iter()
                    .map(|line| line.style(Theme::text_error())),
            );
            return (lines, 0);
        }

        let configurations = self.configurations();
        if configurations.is_empty() {
            let message = if self.launch.is_some() {
                "No configurations"
            } else {
                "No launch.json in .terminalcode"
            };
            let hint = if self.launch.is_some() {
                ""
            } else {
                "Create one to list launch options"
            };

            let max = width.saturating_sub(4) as usize;
            let mut lines = vec![Line::from("")];
            lines.extend(
                wrapped_lines(message, "  ", max)
                    .into_iter()
                    .map(|line| line.style(Theme::text_dim())),
            );
            if !hint.is_empty() {
                lines.push(Line::from(""));
                lines.extend(
                    wrapped_lines(hint, "  ", max)
                        .into_iter()
                        .map(|line| line.style(Theme::text_dim())),
                );
            }
            return (lines, 0);
        }

        let max = width.saturating_sub(4) as usize;
        let cursor = self.scroller.vertical();
        let mut lines = Vec::with_capacity(configurations.len() * 2);
        let mut cursor_visual_line = 0;

        for (i, config) in configurations.iter().enumerate() {
            let selected = i == cursor;
            let mut name_style = Theme::explorer_file();
            let mut program_style = Theme::text_dim();
            if selected {
                Theme::add_highlight(&mut name_style);
                Theme::add_highlight(&mut program_style);
                cursor_visual_line = (i * 2 + 1) as u16;
            }

            lines.push(Line::from(Span::styled(
                format!("  {}", truncate_start(config.name(), max)),
                name_style,
            )));
            lines.push(Line::from(Span::styled(
                format!("    {}", truncate_start(config.program(), max)),
                program_style,
            )));
        }

        (lines, cursor_visual_line)
    }
}

impl Component for Debugger {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: PanelContext) {
        let focused = context == PanelContext::SideBar;

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
            .title(Span::styled(" Debugger ", title_style))
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner_height = area.height.saturating_sub(2);
        let (lines, cursor_visual_line) = self.draw_content(area.width);
        let scroll_offset = self
            .scroller
            .get_scroll(cursor_visual_line, inner_height, 0, 0);

        let paragraph = Paragraph::new(lines)
            .block(block)
            .style(Theme::explorer_bg())
            .scroll((scroll_offset.vertical, scroll_offset.horizontal));

        frame.render_widget(paragraph, area);
    }
}

fn truncate_start(text: &str, max_len: usize) -> String {
    let len = text.chars().count();
    if len <= max_len {
        return text.to_string();
    }

    let keep = max_len.saturating_sub(1);
    let mut truncated = String::with_capacity(max_len + 1);
    truncated.push('…');
    truncated.extend(text.chars().skip(len - keep));
    truncated
}

fn wrapped_lines(text: &str, indent: &str, max: usize) -> Vec<Line<'static>> {
    let indent_len = indent.chars().count();
    let mut lines = Vec::new();
    let mut current = String::from(indent);

    for word in text.split_whitespace() {
        let word = truncate_start(word, max);
        let word_len = word.chars().count();
        let current_len = current.chars().count();

        if current_len == indent_len {
            current.push_str(&word);
        } else if current_len + 1 + word_len <= max {
            current.push(' ');
            current.push_str(&word);
        } else {
            lines.push(Line::from(current));
            current = format!("{indent}{word}");
        }
    }

    if current.chars().count() > indent_len {
        lines.push(Line::from(current));
    }

    lines
}
