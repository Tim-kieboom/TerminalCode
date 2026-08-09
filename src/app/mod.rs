mod components;

use anyhow::Result;
use crossterm::event;
use std::time::Duration;

use crate::{
    StartupArgs,
    app::components::{
        Hideable,
        debug_window::{DebugTag, DebugWindow},
        editor::Editor,
        keybind_display::KeyBindDisplay,
        notifications::{Notification, Notifications},
        sidebar::SideBar,
    },
    keybinds::{Action, KeyBindings, PanelContext},
    terminal::AppTerminal,
};
mod draw;
mod handle_key;

#[cfg(test)]
mod app_tests;

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

    fn key_label(&self, action: Action) -> String {
        match self.keybinds().get(&action, self.context) {
            Some(binding) => binding.to_string(),
            None => "???".into(),
        }
    }

    fn keybinds(&self) -> &KeyBindings {
        &self.keybind_display.inner().keybinds
    }

    fn is_writable(&self) -> bool {
        self.context == PanelContext::Editor
    }

    fn log_error(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.debug_window.inner_mut().push_error(message.clone());
        self.notifications
            .push(Notification::new(DebugTag::Error, message));
    }

    fn log_warning(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.debug_window.inner_mut().push_warning(message.clone());
        self.notifications
            .push(Notification::new(DebugTag::Warning, message));
    }

    fn log_note(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.debug_window.inner_mut().push_note(message.clone());
        self.notifications
            .push(Notification::new(DebugTag::Note, message));
    }
}
