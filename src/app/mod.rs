use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::time::Duration;

use crate::{StartupArgs, keybinds::{Action, KeyBindings}, terminal::AppTerminal};

mod editor;
mod explorer;

pub struct App {
    running: bool,
    selected_panel: SelectedPanel,
    show_keybinds: bool,
    keybinds: KeyBindings,
    args: StartupArgs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectedPanel {
    Explorer,
    Editor,
}

impl App {
    pub fn new(args: StartupArgs) -> Self {
        let keybinds = KeyBindings::load(&args.path);
        Self {
            selected_panel: SelectedPanel::Editor,
            show_keybinds: false,
            running: true,
            keybinds,
            args,
        }
    }

    pub fn run(&mut self, terminal: &mut AppTerminal) -> Result<()> {
        while self.running {
            terminal.draw(|frame| self.draw(frame))?;

            if event::poll(Duration::from_millis(100))? {
                let event = event::read()?;
                self.handle_event(event)?;
            }
        }

        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        let main_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(frame.area());

        self.draw_workspace(frame, main_layout[0]);
        self.draw_status_bar(frame, main_layout[1]);

        if self.show_keybinds {
            self.draw_keybinds(frame, frame.area());
        }
    }

    fn key_label(&self, action: Action) -> String {
        match self.keybinds.get(&action) {
            Some(binding) => binding.to_string(),
            None => "<null>".into(),
        }
    }

    fn draw_status_bar(&self, frame: &mut Frame, area: Rect) {
        let panel = match self.selected_panel {
            SelectedPanel::Explorer => "EXPLORER",
            SelectedPanel::Editor => "EDITOR",
        };

        let quit_key = self.key_label(Action::Quit);
        let keybinds_key = self.key_label(Action::ShowKeyBinds);

        let status = Paragraph::new(format!(
            " {panel} │ {quit_key} Quit │ {keybinds_key} keybinds"
        ));

        frame.render_widget(status, area);
    }

    fn draw_workspace(&self, frame: &mut Frame, area: Rect) {
        let workspace_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(24), Constraint::Min(1)])
            .split(area);

        self.draw_explorer(frame, workspace_layout[0]);
        self.draw_editor(frame, workspace_layout[1]);
    }

    fn draw_keybinds(&self, frame: &mut Frame, area: Rect) {
        let popup_width = 42.min(area.width.saturating_sub(4));
        let num_actions = Action::all().len() as u16;
        let popup_height = (num_actions + 2).min(area.height.saturating_sub(2));

        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length((area.width - popup_width) / 2),
                Constraint::Length(popup_width),
                Constraint::Min(0),
            ])
            .split(area);

        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length((area.height - popup_height) / 2),
                Constraint::Length(popup_height),
                Constraint::Min(0),
            ])
            .split(horizontal[1]);

        let popup_area = vertical[1];

        frame.render_widget(Clear, popup_area);

        let mut lines: Vec<Line> = Vec::new();
        for action in Action::all() {
            let label = self.key_label(*action);
            lines.push(Line::from(format!("  {:<22} {label}", action.description())));
        }

        let block = Block::default()
            .title(" Keybindings ")
            .borders(Borders::ALL)
            .border_style(ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::BOLD));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, popup_area);
    }

    fn handle_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                self.handle_key_event(key);
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) {
        if self.show_keybinds {
            match (key.modifiers, key.code) {
                (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Esc) => {
                    self.show_keybinds = false;
                }
                _ => {}
            }
            return;
        }

        let action = match self.keybinds.resolve(&key) {
            Some(a) => a,
            None => return,
        };

        match action {
            Action::Quit => self.running = false,
            Action::ShowKeyBinds => self.show_keybinds = true,
            Action::ToggleSidebar => self.toggle_sidebar(),
            Action::FocusNextPanel => self.focus_next_panel(),
        }
    }

    fn toggle_sidebar(&mut self) {
        self.selected_panel = match self.selected_panel {
            SelectedPanel::Explorer => SelectedPanel::Editor,
            SelectedPanel::Editor => SelectedPanel::Explorer,
        };
    }

    fn focus_next_panel(&mut self) {
        self.toggle_sidebar();
    }
}
