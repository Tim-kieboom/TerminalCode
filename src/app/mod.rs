mod components;

use anyhow::{Result, bail};
use crossterm::event::{self, Event, KeyEvent, KeyEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::{format, time::Duration};

use crate::{
    StartupArgs,
    app::components::{
        Component, Hideable, debug_window::DebugWindow, editor::Editor,
        keybind_display::KeyBindDisplay, sidebar::SideBar,
    },
    keybinds::{Action, KeyBindings, PanelContext},
    terminal::AppTerminal,
    theme::Theme,
    utils::{horizontal_layout, vertical_layout},
};

const WORKSPACE_HEIGHT: Constraint = Constraint::Min(30);
const STATUSBAR_HEIGHT: Constraint = Constraint::Length(1);

const EDITOR_WIDTH: Constraint = Constraint::Min(1);
const SIDEBAR_WIDTH: Constraint = Constraint::Length(28);

pub struct App {
    running: bool,
    editor: Editor,
    context: PanelContext,
    sidebar: Hideable<SideBar>,
    debug_window: Hideable<DebugWindow>,
    keybind_display: Hideable<KeyBindDisplay>,
}

impl App {
    pub fn new(args: StartupArgs) -> Result<Self> {
        Ok(Self {
            running: true,
            editor: Editor::new(&args),
            context: PanelContext::Editor,
            sidebar: Hideable::new_show(SideBar::new(&args)),
            debug_window: Hideable::new_hide(DebugWindow::new(&args)),
            keybind_display: Hideable::new_hide(KeyBindDisplay::new(&args)?),
        })
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

    fn draw(&mut self, frame: &mut Frame) {
        let layout = vertical_layout([WORKSPACE_HEIGHT, STATUSBAR_HEIGHT], frame.area());

        self.draw_workspace(frame, layout[0]);
        self.draw_status_bar(frame, layout[1]);
        self.debug_window
            .try_draw(frame, frame.area(), self.context);
        self.keybind_display
            .try_draw(frame, frame.area(), self.context);
    }

    fn key_label(&self, action: Action) -> String {
        match self.keybinds().get(&action, self.context) {
            Some(binding) => binding.to_string(),
            None => "???".into(),
        }
    }

    fn keybinds(&self) -> &KeyBindings {
        &self.keybind_display.inner().keybinds
    }

    fn draw_status_bar(&self, frame: &mut Frame, area: Rect) {
        let panel = self.context.description();

        let quit_key = self.key_label(Action::Quit);
        let keybinds_key = self.key_label(Action::ShowKeyBinds);

        let status = Line::from(vec![
            Span::styled(format!(" {panel} "), Theme::status_bar_key()),
            Span::styled(" │ ", Theme::status_bar_dim()),
            Span::styled(quit_key.clone(), Theme::status_bar_key()),
            Span::styled(" Quit ", Theme::status_bar_dim()),
            Span::styled("│ ", Theme::status_bar_dim()),
            Span::styled(keybinds_key, Theme::status_bar_key()),
            Span::styled(" Keybinds ", Theme::status_bar_dim()),
        ]);

        let bar = Paragraph::new(status).style(Theme::status_bar());
        frame.render_widget(bar, area);
    }

    fn draw_workspace(&mut self, frame: &mut Frame, area: Rect) {
        if self.sidebar.should_hide() {
            self.editor.draw(frame, area, self.context);
            return;
        }

        let layout = horizontal_layout([SIDEBAR_WIDTH, EDITOR_WIDTH], area);
        self.sidebar.try_draw(frame, layout[0], self.context);
        self.editor.draw(frame, layout[1], self.context);
    }

    fn handle_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                self.handle_key_event(key)?;
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<()> {
        let action = match self.keybinds().resolve(&key, self.context) {
            Some(a) => a,
            None => return Ok(()),
        };

        match action {
            Action::Quit => self.running = false,
            Action::Close => self.close_window(),
            Action::ToggleSidebar => self.toggle_sidebar(),
            Action::ShowKeyBinds => self.toggle_keybinds(),
            Action::ToggleBottom => self.toggle_bottombar(),
            Action::FocusNextPanel => self.focus_next_panel(),
            Action::ToggleDebugWindow => self.toggle_debug_window(),
            Action::OpenFile => bail!("OpenFile Not yet impl"),
            Action::Test => bail!("test"),

            Action::ScrollUp
            | Action::ScrollTop
            | Action::ScrollDown
            | Action::ScrollLeft
            | Action::ScrollRight
            | Action::ScrollBottom
            | Action::ScrollPageUp
            | Action::ScrollPageDown => self.move_cursor(action),
        }

        Ok(())
    }

    fn move_cursor(&mut self, action: Action) {
        match self.context {
            PanelContext::Editor => self.editor.content.move_curser(action),
            PanelContext::Keybinds => self.keybind_display.inner_mut().move_cursor(action),
            PanelContext::DebugWindow => self.debug_window.inner_mut().move_cursor(action),
            _ => (),
        }
    }

    fn close_window(&mut self) {
        match self.context {
            PanelContext::Keybinds => self.keybind_display.hide(),
            _ => (),
        }
    }

    fn toggle_keybinds(&mut self) {
        self.keybind_display.toggle_hide();

        self.context = if self.keybind_display.should_hide() {
            PanelContext::Editor
        } else {
            PanelContext::Keybinds
        }
    }

    fn toggle_debug_window(&mut self) {
        self.debug_window.toggle_hide();

        self.context = if self.debug_window.should_hide() {
            PanelContext::Editor
        } else {
            PanelContext::DebugWindow
        }
    }

    fn toggle_sidebar(&mut self) {
        self.sidebar.toggle_hide();

        self.context = if self.sidebar.should_hide() {
            PanelContext::Editor
        } else {
            PanelContext::SideBar
        }
    }

    fn toggle_bottombar(&mut self) {
        self.editor.bottombar.toggle_hide();

        self.context = if self.editor.bottombar.should_hide() {
            PanelContext::Editor
        } else {
            PanelContext::BottomBar
        }
    }

    fn focus_next_panel(&mut self) {
        self.context = match self.context {
            PanelContext::Editor => {
                if self.sidebar.should_show() {
                    PanelContext::SideBar
                } else if self.editor.bottombar.should_show() {
                    PanelContext::BottomBar
                } else {
                    PanelContext::Editor
                }
            }
            PanelContext::SideBar => {
                if self.editor.bottombar.should_show() {
                    PanelContext::BottomBar
                } else {
                    PanelContext::Editor
                }
            }
            PanelContext::BottomBar => PanelContext::Editor,
            other => other,
        }
    }
}
