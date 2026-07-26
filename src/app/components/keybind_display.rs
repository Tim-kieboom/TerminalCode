use ratatui::{Frame, layout::{Constraint, Rect}, text::{Line, Span}, widgets::{Block, Borders, Clear, Paragraph}};
use anyhow::Result;
use crate::{StartupArgs, app::components::Component, keybinds::{Action, KeyBindings, PanelContext}, theme::Theme, utils::{horizontal_layout, vertical_layout}};

pub struct KeyBindDisplay {
    pub keybinds: KeyBindings
}
impl KeyBindDisplay {
    pub fn new(args: &StartupArgs) -> Result<Self> {
        let keybinds = KeyBindings::load(&args.path)?;
        Ok(Self { keybinds })
    }

    pub fn scroll(&mut self, action: Action) {
        todo!()
    }
}

impl Component for KeyBindDisplay {
    fn draw(&self, frame: &mut Frame, area: Rect, context: PanelContext) {
        let popup_width = 52.min(area.width.saturating_sub(4));
        
        let global_count = self.keybinds.iter_global().count() as u16;
        let context_count = self.keybinds.iter_context(context).count() as u16;
        
        let total_rows = global_count + context_count + 6;
        let popup_height = total_rows.min(area.height.saturating_sub(2));


        let half_width = (area.width - popup_width) / 2;
        let horizontal = horizontal_layout(
            [
                Constraint::Length(half_width),
                Constraint::Length(popup_width),
                Constraint::Min(0),
            ],
            area
        );

        let half_height = (area.height - popup_height) / 2;
        let vertical = vertical_layout(
            [
                Constraint::Length(half_height),
                Constraint::Length(popup_height),
                Constraint::Min(0),
            ], 
            horizontal[1],
        );

        let popup_area = vertical[1];
        frame.render_widget(Clear, popup_area);

        let mut lines: Vec<Line> = Vec::new();

        lines.push(Line::from(""));

        // Global section
        lines.push(Line::from(vec![
            Span::styled("    Global", Theme::text_accent()),
        ]));

        for (action, binding) in self.keybinds.iter_global() {
            lines.push(Line::from(vec![
                Span::styled("      ", Theme::text_dim()),
                Span::styled(format!("{:<22}", action.description()), Theme::keybind_action()),
                Span::styled(binding.to_string(), Theme::keybind_key()),
            ]));
        }

        // Context section
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(format!("    {}", context.description()), Theme::text_accent()),
        ]));

        for (action, binding) in self.keybinds.iter_context(context) {
            lines.push(Line::from(vec![
                Span::styled("      ", Theme::text_dim()),
                Span::styled(format!("{:<22}", action.description()), Theme::keybind_action()),
                Span::styled(binding.to_string(), Theme::keybind_key()),
            ]));
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

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, popup_area);
    }
}