use anyhow::Result;
use ratatui::{Frame, widgets::Block};

use crate::{
    key_controller::{InsertKind, KeyController, KeyDoneKind},
    window::{command_prompt::CommandPrompt, lookup_bar::LookupBar, text_editor::TextEditor},
};

pub mod lookup_bar;
pub mod text_editor;
pub mod command_prompt;

macro_rules! impl_window_for_enum {
    ($enum_name:ident { $($variant:ident),* $(,)? }) => {
        impl KeyController for $enum_name {
            fn move_up(&mut self) -> Result<KeyDoneKind> {
              match self {
                    $(
                        $enum_name::$variant(inner) => inner.move_up(),
                    )*
                }
            }
            fn move_down(&mut self) -> Result<KeyDoneKind> {
              match self {
                    $(
                        $enum_name::$variant(inner) => inner.move_down(),
                    )*
                }
            }
            fn move_left(&mut self, amount: u16) -> Result<KeyDoneKind> {
              match self {
                    $(
                        $enum_name::$variant(inner) => inner.move_left(amount),
                    )*
                }
            }
            fn move_right(&mut self, amount: u16) -> Result<KeyDoneKind> {
              match self {
                    $(
                        $enum_name::$variant(inner) => inner.move_right(amount),
                    )*
                }
            }
            fn enter(&mut self) -> Result<KeyDoneKind> {
              match self {
                    $(
                        $enum_name::$variant(inner) => inner.enter(),
                    )*
                }
            }
            fn backspace(&mut self) -> Result<KeyDoneKind> {
              match self {
                    $(
                        $enum_name::$variant(inner) => inner.backspace(),
                    )*
                }
            }
            fn insert(&mut self, insert: InsertKind) -> Result<KeyDoneKind> {
              match self {
                    $(
                        $enum_name::$variant(inner) => inner.insert(insert),
                    )*
                }
            }
        }

        impl Window for $enum_name {
            fn on_insert(&mut self) {
                match self {
                    $(
                        $enum_name::$variant(inner) => inner.on_insert(),
                    )*
                }
            }
            fn on_remove(&mut self) {
                match self {
                    $(
                        $enum_name::$variant(inner) => inner.on_insert(),
                    )*
                }
            }
            fn draw_ui(&mut self, frame: &mut Frame, header: Block) {
                match self {
                    $(
                        $enum_name::$variant(inner) => inner.draw_ui(frame, header),
                    )*
                }
            }
        }
    };
}

pub trait Window: KeyController {
    fn on_insert(&mut self);
    fn on_remove(&mut self);
    fn draw_ui(&mut self, frame: &mut Frame, header: Block);
}

#[derive(Debug)]
pub enum WindowKind {
    LookupBar(LookupBar),
    TextEditor(TextEditor),
    CommandPrompt(CommandPrompt)
}
impl_window_for_enum!(WindowKind {
    LookupBar,
    TextEditor,
    CommandPrompt
});
