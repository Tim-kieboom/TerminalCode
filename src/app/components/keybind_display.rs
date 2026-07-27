use std::vec;

use ratatui::{
    Frame, layout::{Rect}, style::Style, text::{Line, Span}, widgets::{Block, Borders, Clear, Paragraph},
};
use anyhow::Result;
use crate::{
    StartupArgs, app::components::Component, keybinds::{Action, KeyBinding, KeyBindings, PanelContext}, theme::Theme, utils::popup_layout,
};

const MIN_WIDTH: u16 = 56;

pub struct KeyBindDisplay {
    pub keybinds: KeyBindings,
    cursor: usize,
}

impl KeyBindDisplay {
    pub fn new(args: &StartupArgs) -> Result<Self> {
        let keybinds = KeyBindings::load(&args.path)?;
        Ok(Self {
            keybinds,
            cursor: 0,
        })
    }

    pub fn move_cursor(&mut self, action: Action) {
        let total = self.total_items();
        if total == 0 {
            return;
        }
        match action {
            Action::ScrollUp => {
                self.cursor = self.cursor.saturating_sub(1);
            }
            Action::ScrollDown => {
                self.cursor = self.cursor.saturating_add(1).min(total - 1);
            }
            Action::ScrollPageUp => {
                self.cursor = self.cursor.saturating_sub(10);
            }
            Action::ScrollPageDown => {
                self.cursor = self.cursor.saturating_add(10).min(total - 1);
            }
            Action::ScrollTop => {
                self.cursor = 0;
            }
            Action::ScrollBottom => {
                self.cursor = total - 1;
            }
            _ => {}
        }
    }

    fn total_items_for(&self, context: PanelContext) -> (u16, u16) {
        let global = self.keybinds.iter_global().count() as u16;
        let ctx = self.keybinds.get_context_map(context).map(|map| map.len()).unwrap_or(0) as u16;
        (global, ctx)
    }

    fn total_items(&self) -> usize {
        self.keybinds.iter_global().count() + self.keybinds.get_context_map(PanelContext::Keybinds).map(|map| map.len()).unwrap_or(0)
    }

    fn cursor_line(&self, global_count: u16) -> u16 {
        if (self.cursor as u16) < global_count {
            2 + self.cursor as u16
        } else {
            4 + self.cursor as u16
        }
    }

    fn scroll_offset(cursor_line: u16, inner_height: u16) -> u16 {
        if cursor_line < inner_height {
            0
        } else {
            cursor_line - inner_height + 1
        }
    }
}

impl Component for KeyBindDisplay {
    fn draw(&self, frame: &mut Frame, area: Rect, context: PanelContext) {
        let popup_width = MIN_WIDTH.min(area.width.saturating_sub(4));

        let (global_count, context_count) = self.total_items_for(context);
        let total = global_count + context_count + 6;
        let popup_height = total.min(area.height.saturating_sub(2));
        let inner_height = popup_height.saturating_sub(2);
        let layout = popup_layout(area, popup_width, popup_height);

        let popup_area = layout[1];
        frame.render_widget(Clear, popup_area);

        let cursor_line = self.cursor_line(global_count);
        let offset = Self::scroll_offset(cursor_line, inner_height);

        let mut lines = vec![
            Line::from(""),
            Line::styled("    Global", Theme::text_accent())
        ];

        for (i, (action, binding)) in self.keybinds.iter_global().enumerate() {
            let selected = i == self.cursor;

            let mut styles = [
                Theme::keybind_action(),
                Theme::keybind_key(),
                Theme::text_dim(),
            ];

            if selected {
                for style in &mut styles {
                    Theme::add_highlight(style);
                }
            }

            let [action_style, key_style, prefix_style] = styles;

            lines.push(Line::from(vec![
                Span::styled("  ", prefix_style),
                Span::styled(
                    format!("{} ", if selected { ">" } else { " " }),
                    prefix_style,
                ),
                Span::styled(format!("{:<22}", action.description()), action_style),
                Span::styled(binding.to_string(), key_style),
            ]));
        }

        for (context, map) in self.keybinds.iter_contexts() {
            lines.push(Line::from(""));
            lines.push(Line::styled(format!("    {}", context.description()), Theme::text_accent()));

            for (i, (action, binding)) in map.iter().enumerate() {                
                let line = self.context_line(action, binding, global_count, i);
                lines.push(line)
            } 
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("    ", Theme::text_dim()),
            Span::styled("Press ", Theme::keybind_dim()),
            Span::styled("Esc", Theme::keybind_key()),
            Span::styled(" to close", Theme::keybind_dim()),
        ]));

        let block = Block::default()
            .title(Span::styled(" Keybindings ", Theme::popup_title()))
            .borders(Borders::ALL)
            .border_style(Theme::popup_border());

        let paragraph = Paragraph::new(lines)
            .block(block)
            .scroll((offset, 0));

        frame.render_widget(paragraph, popup_area);
    }
}


impl KeyBindDisplay {
    pub fn context_line<'a>(&self, action: &Action, binding: &KeyBinding, global_count: u16, i: usize) -> Line<'a> {
        const LINE_SELECTED: [Style; 3] = [Theme::keybind_action_selected(), Theme::keybind_key_selected(), Theme::keybind_selected()];
        const LINE_DEFAULT: [Style; 3] = [Theme::keybind_action(), Theme::keybind_key(), Theme::text_dim()];
        
        let idx = global_count as usize + i;
        let selected = idx == self.cursor;
        let [action_style, key_style, prefix_style] = if selected {
            LINE_SELECTED
        } else {
            LINE_DEFAULT
        };

        let select = if selected { "> " } else { "  " };
        Line::from(vec![
            Span::styled("  ", prefix_style),
            Span::styled(select, prefix_style),
            Span::styled(format!("{:<22}", action.description()), action_style),
            Span::styled(binding.to_string(), key_style),
        ])
    }
}
