use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};

use crate::{
    App,
    app::components::editor::bottombar::terminal_keys,
    keybinds::{Action, KeyBinding, PanelContext},
};

impl App {
    pub(super) fn handle_event(&mut self, event: Event) -> Result<()> {
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
        if self.context == PanelContext::BottomBar {
            return self.handle_terminal_key(key);
        }

        let action = match self.keybinds().resolve(&key, self.context) {
            Some(a) => a,
            None => {
                self.log_warning(format!("unhandled key {}", KeyBinding::from(key)));
                return Ok(());
            }
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
            Action::Delete => self.editor.delete_char(),
            Action::Backspace => self.editor.backspace(),
            Action::InsertTab => self.editor.insert_tab(),
            Action::InsertNewline => self.editor.insert_newline(),
            Action::Save => self.save(),

            Action::ScrollUp
            | Action::ScrollTop
            | Action::ScrollDown
            | Action::ScrollLeft
            | Action::ScrollRight
            | Action::ScrollWordLeft
            | Action::ScrollWordRight
            | Action::ScrollBottom
            | Action::ScrollPageUp
            | Action::ScrollPageDown => self.move_cursor(action),
            Action::PrevTab => self.switch_tab(-1),
            Action::NextTab => self.switch_tab(1),

            Action::SwitchBottom => self.switch_bottom(),
            Action::SwitchSidebar => self.switch_sidebar(),
        }

        Ok(())
    }

    fn handle_terminal_key(&mut self, key: KeyEvent) -> Result<()> {
        match self.keybinds().resolve_global(&key) {
            Some(Action::Quit) => self.running = false,
            Some(Action::Close) => self.close_window(),
            Some(Action::SwitchBottom) => self.switch_bottom(),
            Some(Action::SwitchSidebar) => self.switch_sidebar(),
            Some(Action::ToggleSidebar) => self.toggle_sidebar(),
            Some(Action::ShowKeyBinds) => self.toggle_keybinds(),
            Some(Action::ToggleBottom) => self.toggle_bottombar(),
            Some(Action::FocusNextPanel) => self.focus_next_panel(),
            Some(Action::ToggleDebugWindow) => self.toggle_debug_window(),
            _ => match terminal_keys::encode_key(&key) {
                Some(bytes) => self.editor.bottombar.inner_mut().write_input(&bytes),
                None => self.log_warning(format!("unhandled key {}", KeyBinding::from(key))),
            },
        }

        Ok(())
    }

    fn switch_tab(&mut self, amount: isize) {
        match self.context {
            PanelContext::Editor => self.editor.switch_tab(amount),
            PanelContext::SideBar
            | PanelContext::Keybinds
            | PanelContext::BottomBar
            | PanelContext::DebugWindow => self.log_warning(format!(
                "{} does not implement tab switching",
                self.context.description()
            )),
        }
    }

    fn save(&mut self) {
        match self.editor.save_active() {
            Ok(true) => self.log_note("Saved".to_string()),
            Ok(false) => {}
            Err(err) => self.log_error(format!("Failed to save: {err}")),
        }
    }

    fn insert_char(&mut self, key: KeyEvent) {
        if !self.is_writable() {
            return;
        }

        let KeyCode::Char(ch) = key.code else {
            self.log_error(format!("{} should be unreachable in InsertChar", key.code,));
            return;
        };

        self.editor.insert_char(ch)
    }

    fn move_cursor(&mut self, action: Action) {
        match self.context {
            PanelContext::Editor => self.editor.content.move_cursor(action),
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

    fn switch_bottom(&mut self) {
        self.editor.bottombar.show();

        self.context = if self.context == PanelContext::BottomBar {
            PanelContext::Editor
        } else {
            PanelContext::BottomBar
        }
    }

    fn switch_sidebar(&mut self) {
        self.sidebar.show();

        self.context = if self.context == PanelContext::SideBar {
            PanelContext::Editor
        } else {
            PanelContext::SideBar
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
}
