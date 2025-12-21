use crate::{
    key_controller::{
        InsertKind, WindowControlReponse, WindowsControl, key_controller::SessionEvent,
    }, utils::syntaxer::Syntaxer, window::{
        command_prompt::CommandPrompt, file_creater::FileCreater, filetree_window::FileTreeWindow, lookup_bar::LookupBar, notification_window::NotificationWindow, text_editor::TextEditor
    }
};
use anyhow::Result;
use ratatui::{Frame, widgets::Block};

pub mod command_prompt;
pub mod file_creater;
pub mod filetree_window;
pub mod lookup_bar;
pub mod notification_window;
pub mod text_editor;

macro_rules! impl_window_for_enum {
    ($enum_name:ident { $($variant:ident),* $(,)? }) => {
        impl WindowsControl for $enum_name {
            fn move_up(&mut self) -> Result<WindowControlReponse> {
                match self {
                    $(
                        $enum_name::$variant(inner) => inner.move_up(),
                    )*
                }
            }
            fn move_down(&mut self) -> Result<WindowControlReponse> {
                match self {
                    $(
                        $enum_name::$variant(inner) => inner.move_down(),
                    )*
                }
            }
            fn move_left(&mut self, amount: u16) -> Result<WindowControlReponse> {
                match self {
                    $(
                        $enum_name::$variant(inner) => inner.move_left(amount),
                    )*
                }
            }
            fn move_right(&mut self, amount: u16) -> Result<WindowControlReponse> {
                match self {
                    $(
                        $enum_name::$variant(inner) => inner.move_right(amount),
                    )*
                }
            }
            fn enter(&mut self) -> Result<WindowControlReponse> {
                match self {
                    $(
                        $enum_name::$variant(inner) => inner.enter(),
                    )*
                }
            }
            fn backspace(&mut self) -> Result<WindowControlReponse> {
                match self {
                    $(
                        $enum_name::$variant(inner) => inner.backspace(),
                    )*
                }
            }
            fn insert(&mut self, insert: InsertKind) -> Result<WindowControlReponse> {
                match self {
                    $(
                        $enum_name::$variant(inner) => inner.insert(insert),
                    )*
                }
            }
            fn custom_action(&mut self, action: crossterm::event::Event) -> Result<Option<SessionEvent>> {
                match self {
                    $(
                        $enum_name::$variant(inner) => inner.custom_action(action),
                    )*
                }
            }
        }

        impl Window for $enum_name {
            fn on_insert(&mut self) -> Result<()> {
                match self {
                    $(
                        $enum_name::$variant(inner) => inner.on_insert(),
                    )*
                }
            }
            fn on_remove(&mut self) -> Result<()> {
                match self {
                    $(
                        $enum_name::$variant(inner) => inner.on_insert(),
                    )*
                }
            }
            fn draw_ui(&mut self, frame: &mut Frame, header: Block, syntaxer: &mut Syntaxer) -> Result<()> {
                match self {
                    $(
                        $enum_name::$variant(inner) => inner.draw_ui(frame, header, syntaxer),
                    )*
                }
            }
        }
    };
}

pub trait Window: WindowsControl {
    fn on_insert(&mut self) -> Result<()>;
    fn on_remove(&mut self) -> Result<()>;
    fn draw_ui(&mut self, frame: &mut Frame, header: Block, syntaxer: &mut Syntaxer) -> Result<()>;
}

#[derive(Debug)]
pub enum WindowKind {
    LookupBar(LookupBar),
    TextEditor(TextEditor),
    FileCreater(FileCreater),
    CommandPrompt(CommandPrompt),
    FileTreeWindow(FileTreeWindow),
    NotificationWindow(NotificationWindow),
}
impl_window_for_enum!(WindowKind {
    LookupBar,
    TextEditor,
    FileCreater,
    CommandPrompt,
    FileTreeWindow,
    NotificationWindow,
});
