mod components;

use anyhow::{Result, bail};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};
use std::{format, time::Duration, vec};

use crate::{
    StartupArgs,
    app::components::{
        Component, Hideable,
        debug_window::{DebugTag, DebugWindow},
        editor::Editor,
        keybind_display::KeyBindDisplay,
        notifications::Notifications,
        sidebar::SideBar,
    },
    keybinds::{Action, KeyBindings, PanelContext},
    layout::{EDITOR_WIDTH, SIDEBAR_WIDTH, STATUSBAR_HEIGHT, WORKSPACE_HEIGHT},
    terminal::AppTerminal,
    theme::Theme,
    utils::{horizontal_layout, vertical_layout},
};

pub struct App {
    running: bool,
    editor: Editor,
    context: PanelContext,
    sidebar: Hideable<SideBar>,
    debug_window: Hideable<DebugWindow>,
    keybind_display: Hideable<KeyBindDisplay>,
    notifications: Notifications,
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
            notifications: Notifications::new(),
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
        self.notifications.update();

        let layout = vertical_layout([WORKSPACE_HEIGHT, STATUSBAR_HEIGHT], frame.area());

        self.draw_workspace(frame, layout[0]);
        self.draw_status_bar(frame, layout[1]);
        self.debug_window
            .try_draw(frame, frame.area(), self.context);
        self.keybind_display
            .try_draw(frame, frame.area(), self.context);
        self.notifications.draw(frame, frame.area());
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

        let bar = Paragraph::new(vec![Line::from(""), status]).style(Theme::status_bar());
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
            Event::Key(_)
            | Event::Paste(_)
            | Event::Mouse(_)
            | Event::FocusLost
            | Event::FocusGained
            | Event::Resize(_, _) => {}
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
            Action::OpenFile => self.open_file(),
            Action::Test => self.log_note("test".to_string()),

            Action::InsertChar => self.insert_char(key),
            Action::Delete => self.editor.content.delete_char(),
            Action::Backspace => self.editor.content.backspace(),
            Action::InsertTab => self.editor.content.insert_tab(),
            Action::InsertNewline => self.editor.content.insert_newline(),
            Action::Save => bail!("Save Not yet impl"),

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

    fn is_writable(&self) -> bool {
        self.context == PanelContext::Editor
    }

    fn insert_char(&mut self, key: KeyEvent) {
        if !self.is_writable() {
            return;
        }

        let KeyCode::Char(ch) = key.code else {
            self.log_error(format!("{} should be unreachable in InsertChar", key.code,));
            return;
        };

        self.editor.content.insert_char(ch)
    }

    fn move_cursor(&mut self, action: Action) {
        match self.context {
            PanelContext::Editor => self.editor.content.move_curser(action),
            PanelContext::SideBar => self.sidebar.inner_mut().move_cursor(action),
            PanelContext::Keybinds => self.keybind_display.inner_mut().move_cursor(action),
            PanelContext::DebugWindow => self.debug_window.inner_mut().move_cursor(action),
            _ => (),
        }
    }

    fn open_file(&mut self) {
        match self.context {
            PanelContext::SideBar => {
                let Some(path) = self.sidebar.inner_mut().open_current() else {
                    return;
                };

                match self.editor.open(&path) {
                    Ok(()) => self.context = PanelContext::Editor,
                    Err(err) => self.log_error(format!("Failed to open {}: {err}", path.display())),
                }
            }
            _ => self.log_error("OpenFile is only available in the sidebar"),
        }
    }

    fn close_window(&mut self) {
        if self.context == PanelContext::Keybinds {
            self.keybind_display.hide();
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
        };
    }

    #[allow(unused)]
    fn log_error(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.debug_window.inner_mut().push_error(message.clone());
        self.notifications.push(DebugTag::Error, message);
    }

    #[allow(unused)]
    fn log_warning(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.debug_window.inner_mut().push_warning(message.clone());
        self.notifications.push(DebugTag::Warning, message);
    }

    #[allow(unused)]
    fn log_note(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.debug_window.inner_mut().push_note(message.clone());
        self.notifications.push(DebugTag::Note, message);
    }
}
