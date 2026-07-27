use std::vec;

use crate::{
    StartupArgs,
    app::components::{
        Component,
        utils::cursor_scroller::{CursorScroller, ScrollMode},
    },
    keybinds::{Action, KeyBinding, KeyBindings, PanelContext},
    theme::Theme,
    utils::popup_layout,
};
use anyhow::Result;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

const MIN_WIDTH: u16 = 56;

pub struct KeyBindDisplay {
    pub keybinds: KeyBindings,
    scroller: CursorScroller,
}

impl KeyBindDisplay {
    pub fn new(args: &StartupArgs) -> Result<Self> {
        let keybinds = KeyBindings::load(&args.path)?;
        Ok(Self {
            keybinds,
            scroller: CursorScroller::new(ScrollMode::List),
        })
    }

    pub fn move_cursor(&mut self, action: Action) {
        let length = self.total_items();
        self.scroller.move_cursor(action, length);
    }

    fn total_items_for(&self, context: PanelContext) -> (u16, u16) {
        let global = self.keybinds.iter_global().count() as u16;
        let ctx = self
            .keybinds
            .get_context_map(context)
            .map(|map| map.len())
            .unwrap_or(0) as u16;
        (global, ctx)
    }

    fn total_items(&self) -> usize {
        let globals = self.keybinds.iter_global().count();
        let contexts: usize = self
            .keybinds
            .iter_contexts()
            .map(|(_, map)| map.len())
            .sum();
        globals + contexts
    }
}

impl Component for KeyBindDisplay {
    fn draw(&mut self, frame: &mut Frame, area: Rect, context: PanelContext) {
        let popup_width = MIN_WIDTH.min(area.width.saturating_sub(4));

        let (global_count, context_count) = self.total_items_for(context);
        let length = global_count + context_count + 6;
        let popup_height = length.min(area.height.saturating_sub(2));
        let inner_height = popup_height.saturating_sub(2);
        let popup_area = popup_layout(area, popup_width, popup_height);

        frame.render_widget(Clear, popup_area);

        let scroll_offset = self.scroller.get_scroll(length, inner_height);

        let mut lines = vec![
            Line::from(""),
            Line::styled("    Global", Theme::text_accent()),
        ];

        for (i, (action, binding)) in self.keybinds.iter_global().enumerate() {
            let selected = i == self.scroller.cursor();

            let [action_style, key_style, prefix_style] = if selected {
                [
                    Theme::keybind_action(),
                    Theme::keybind_key(),
                    Theme::text_dim(),
                ]
            } else {
                [
                    Theme::into_highlight(Theme::keybind_action()),
                    Theme::into_highlight(Theme::keybind_key()),
                    Theme::into_highlight(Theme::text_dim()),
                ]
            };

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
            lines.push(Line::styled(
                format!("    {}", context.description()),
                Theme::text_accent(),
            ));

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

        let paragraph = Paragraph::new(lines).block(block).scroll(scroll_offset);

        frame.render_widget(paragraph, popup_area);
    }
}

impl KeyBindDisplay {
    pub fn context_line<'a>(
        &self,
        action: &Action,
        binding: &KeyBinding,
        global_count: u16,
        i: usize,
    ) -> Line<'a> {
        let idx = global_count as usize + i;
        let selected = idx == self.scroller.cursor();
        let [action_style, key_style, prefix_style] = if selected {
            [
                Theme::into_highlight(Theme::keybind_action()),
                Theme::into_highlight(Theme::keybind_key()),
                Theme::into_highlight(Theme::text_dim()),
            ]
        } else {
            [
                Theme::keybind_action(),
                Theme::keybind_key(),
                Theme::text_dim(),
            ]
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
